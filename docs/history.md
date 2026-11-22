# Check-in graph paths

The `gossil::history` module finds paths and common ancestors in caller-supplied check-in
rows. Use it to inspect graph behavior in memory with finite input and search
limits. Its algorithms come from Fossil's `path.c` and `pqueue.c`. On this page,
"native" means those algorithms and their queue order.

The caller supplies the graph and resolves its branch and time metadata. This
module opens no database and grants no permission to read repository history.
The broader history API remains partial. Persistence, branch management and sync
remain separate work.

## Supply ordered rows

`Graph::new` borrows a slice of `Checkin` metadata and indexes the supplied links.
Each check-in needs a unique positive `rid`, branch bytes without NUL and a finite
Julian-day time. A Julian day is the numeric count of days that Fossil uses for time.
The caller resolves native branch defaults and missing times before construction.

`ParentLink` supplies a parent, child and primary-parent flag for each `plink`
row. `Cherrypick` supplies a source, destination and exclusion flag for each
`cherrypick` row. Both endpoints must occur in the check-in slice.

Keep the native query order of both slices of rows, with their duplicates and
excluded cherrypicks. Construction keeps the row order of each direction, and it
sorts only a separate index of identifiers. A new order of the rows can change which
path of equal cost a search returns. The retained
[native capture script](https://github.com/rahuldave/gossil/blob/main/tests/fixtures/history/capture-native.py)
queries child and parent `plink` rows without `ORDER BY`. The field `query_rows` in the
[recorded fixture](https://github.com/rahuldave/gossil/blob/main/tests/fixtures/history/native.json)
retains the observed order. This graph API imposes no database query order.

## Choose a search

| Call | Result and purpose |
|---|---|
| `Graph::new(nodes, links, cherries, limits)` | Validate metadata and index ordered graph rows. |
| `graph.shortest(from, to, options)` | Return an owned `Path` between two known check-ins, or `Ok(None)` when the search finds no path. |
| `graph.common_ancestor(me, you)` | Return an `Ancestor` with a shared `pivot` and a diagnostic path, or `Ok(None)` when the search finds no ancestor. |

For `shortest`, set these `Options` explicitly:

| Option | Behavior |
|---|---|
| `primary_only` | Skip non-primary parent links. |
| `direction: Direction::Both` | Search backwards until the first forward step, then forwards only. |
| `direction: Direction::Forward` | Search from parent to child only. |
| `direction: Direction::ForwardWithCherrypicks` | Also permit a non-excluded cherrypick as the first edge only. |
| `hidden` | Keep these known identifiers traversable and charge their cost, but exclude hidden steps from bisect counts. |
| `branch_cost: 0` | Charge one per node, including the starting node. |
| `branch_cost: positive` | Start at zero, then charge absolute time distance plus this penalty for each branch change. |

The starting node remains visible even when its identifier occurs in `hidden`.
A positive branch cost must fit `i32`. A search with a positive branch cost can
return a path with more edges than the unweighted path, because it minimizes a
different cost.

The native queue does not take equal costs in first-in, first-out (FIFO) order. On
insertion, a heap parent with an equal cost stays in place. On removal, the queue
swaps through equal costs and prefers the left child on a tie. A shortest-path
search visits the child rows before the backwards rows. These rules and the order
of the input rows decide each tie.

`common_ancestor` searches backwards from both endpoints. If either endpoint is
zero, it returns `Ok(None)`. If the two endpoints are equal, known and positive, it
returns a path of one element. The returned path can start at either requested
endpoint, because the native order of discovery sets its orientation.

## Read the result

`Path.steps` includes both endpoints. Each `Step` contains its identifier, a
`from_is_parent` direction flag and its hidden status. `Path.cost` contains the
native cumulative cost.

For a shortest path, `from_is_parent` is true when the preceding step is the
parent of this step. The first step has no preceding edge. Ancestor paths keep
the native rewiring behavior, which can leave a direction flag false. So the flags of an
ancestor path do not say whether the preceding step is the parent.

`Path::visible_steps` counts non-hidden elements after the starting element.
`midpoint` counts backwards from the destination, and it returns no element when
the path has fewer than two visible steps. `search_depth` estimates the number of
bisect comparisons that remain as the ceiling of log2 of the visible count. It
returns zero for a count below two.

These helpers describe the returned path. They do not implement a persisted
bisect session or authorize access to its elements.

## Bound inputs and searches

There is no default `Limits` value. Choose every bound for the application:

| Field | Bound |
|---|---|
| `checkins` | Number of supplied check-ins; also limits the length of `hidden`. |
| `links` | Combined number of parent and cherrypick rows, including excluded rows. |
| `branch_bytes` | Sum of all supplied branch byte lengths. |
| `search_nodes` | Allocated search records, including duplicate pending records. |
| `row_visits` | Adjacency rows inspected by each search, including filtered rows. |

The bounds limit counts and traversal, and not the total bytes on the heap.

`Error::Limit` reports exhausted bounds. `InvalidGraph` reports malformed
metadata, unknown identifiers or a cyclic chain of predecessors inside a search.
`NonFiniteCost` reports an overflow in a time difference or in the cumulative cost.
`Allocation` keeps the typed error of the failed reservation. A failed search
returns no partial path. When the search finds no path, it succeeds with `None`.

Graph validation checks the shape of the supplied data. Derived `plink` rows alone
cannot establish provenance, private membership or complete history. A future
storage adapter needs its own checks of admission and scope before it supplies rows.

## Provenance and tests

The port follows Fossil check-in `7e8ecf0ce4`, the same check-in as the
reference binary of the project. The sources are in `src/path.c` and
`src/pqueue.c`:

| Native function | Lines | Port |
|---|---|---|
| `path_new_node` | `src/path.c` lines 106 to 138 | `Search::add` |
| `path_shortest` | `src/path.c` lines 184 to 271 | `Graph::shortest` |
| `path_midpoint` | `src/path.c` lines 277 to 285 | `Path::midpoint` |
| `path_search_depth` | `src/path.c` lines 313 to 317 | `Path::search_depth` |
| `path_common_ancestor` | `src/path.c` lines 396 to 447 | `Graph::common_ancestor` |
| `pqueuex_rebalance` | `src/pqueue.c` lines 99 to 109 | `Search::add` |
| `pqueuex_pop` | `src/pqueue.c` lines 131 to 146 | `Search::pop` |

The caller resolves branches as `branch_of_rid` does in `src/branch.c`, lines
57 to 73, and times as `mtime_of_rid` does in `src/descendants.c`, lines 166
to 175. The record `evidence/history/source-correspondence.json` maps each
function of `path.c` to its port or says why it is not ported. It also lists
each line of the native source that calls a function of `path.c`, and the
SHA256 of each cited source file. The record holds 17 lines for the three
search functions and 16 lines in other files for the other functions.

The fixtures under `tests/fixtures/history/` hold the Fossil license and two
captures, with the scripts that made them. The Fossil 2.29 reference binary made
both captures over a synthetic repository:

- `native.json` holds 16 runs of `test-shortest-path` and 9 runs of
  `test-ancestor-path`, with their commands, outputs and parsed paths.
- `native-queue.json` holds one run of `test-pqueue` with five equal costs.

A fresh capture with the same binary gave the same paths, pivots and queue
order. Only the random identity of the scratch repository differed. The
`test-shortest-path` command has no option for the first-hop cherrypick
direction, so `Direction::ForwardWithCherrypicks` has no native capture. A Rust
test checks it against `src/path.c` lines 226 to 240.

The tests in `tests/history.rs` replay all 25 captured cases. They also check
the controls that the port adds for bounds, malformed input, cost overflow,
cycles and endpoint orientation. Three unit tests in `src/history.rs` check the typed allocation
error, the native queue order and the error for a cyclic chain of predecessors.
The doc comments of `Graph::shortest` and `Graph::common_ancestor` hold two
doctests. The inherited Tcl tests of the pinned
check-in do not call either diagnostic, so no inherited assertion applies.
These tests do not qualify Gossil repository access or the native callers in
bisect, annotation, file history and timeline.

## Recommendations

Use an explicit graph with finite bounds, and keep the native order of the query rows.
Next, qualify storage admission separately, before you connect persisted history.
