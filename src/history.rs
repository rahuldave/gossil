//! Port the check-in graph searches of Fossil `src/path.c` and `src/pqueue.c`, with finite bounds.
//!
//! The original code is copyright 2011 D. Richard Hipp, under the 2-Clause BSD
//! license. The license text is in `tests/fixtures/history/fossil-license.txt`.
//!
//! The module ports `path_shortest`, `path_common_ancestor` and the queue order of
//! `pqueue.c` from Fossil check-in `7e8ecf0ce4`, over rows that the caller supplies.
//! It neither reads a repository nor grants authority to derived indexes.
//! Callers supply the query order of the `plink` and `cherrypick` rows, and the
//! resolved metadata of branch and time. The algorithms keep the tie order of
//! `pqueue.c`, and the native ban on a backwards step after a forward edge. The
//! bounds and the errors for malformed input are new controls.
//!
//! # Recommendations
//! Use explicit finite limits, and keep the native query order of the rows.
//! Next, qualify a separately authorized storage adapter before using persisted rows.
use std::collections::TryReserveError;
use std::fmt;

/// A `Checkin` holds one check-in with resolved metadata.
///
/// The caller resolves the default branch and the default for a missing time.
#[derive(Clone, Copy, Debug)]
pub struct Checkin<'a> {
    /// Positive native record identifier.
    pub rid: i32,
    /// Resolved `branch_of_rid` value, without an embedded NUL.
    pub branch: &'a [u8],
    /// Finite Julian-day value of `mtime_of_rid`. The native default for a missing time is zero.
    pub mtime: f64,
}
/// A `ParentLink` holds one `plink` row, in the caller's native query order.
#[derive(Clone, Copy, Debug)]
pub struct ParentLink {
    /// Parent check-in identifier.
    pub parent: i32,
    /// Child check-in identifier.
    pub child: i32,
    /// Native `isprim` flag.
    pub primary: bool,
}
/// A `Cherrypick` holds one `cherrypick` row, and excluded rows are kept.
#[derive(Clone, Copy, Debug)]
pub struct Cherrypick {
    /// Source check-in identifier.
    pub parent: i32,
    /// Destination check-in identifier.
    pub child: i32,
    /// Native `isExclude` flag.
    pub excluded: bool,
}
/// `Limits` sets finite bounds on the input and on each search. The type has no unbounded default.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum check-ins.
    pub checkins: usize,
    /// Maximum combined parent and cherrypick rows.
    pub links: usize,
    /// Maximum total bytes of branch metadata.
    pub branch_bytes: usize,
    /// Maximum allocated search nodes, including duplicate pending records.
    pub search_nodes: usize,
    /// Maximum adjacency rows inspected by one search.
    pub row_visits: usize,
}
/// A `Direction` of search matches one of the native `oneWayOnly` values 0, 1 and 2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// Permit backwards steps only until the first forward step.
    Both,
    /// Search from parent to child only.
    Forward,
    /// Search forward, and also follow a non-excluded cherrypick from the initial node only.
    ForwardWithCherrypicks,
}
/// `Options` sets the flags of the native shortest-path search.
#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    /// Ignore non-primary `plink` rows.
    pub primary_only: bool,
    /// Permitted edge direction.
    pub direction: Direction,
    /// Hidden nodes remain traversable and count toward cost.
    pub hidden: &'a [i32],
    /// Nonnegative native cost of a branch change. Zero minimizes the number of edges.
    pub branch_cost: u32,
}
/// A `Step` is one element of a returned path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Step {
    /// Native record identifier.
    pub rid: i32,
    /// Native flag that the preceding step is the parent.
    ///
    /// The rewiring of an ancestor path leaves it false.
    pub from_is_parent: bool,
    /// Whether the element is hidden in the shortest-path result.
    pub hidden: bool,
}
/// A `Path` is an owned path, with no native global mutable state.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    /// Ordered elements, including both endpoints.
    pub steps: Vec<Step>,
    /// Native cumulative cost, including the initial cost of one when unweighted.
    pub cost: f64,
}
impl Path {
    /// Count non-hidden elements after the starting element, as native bisect does.
    /// # Returns
    /// Number of eligible steps.
    pub fn visible_steps(&self) -> usize {
        self.steps.iter().skip(1).filter(|s| !s.hidden).count()
    }
    /// Find the native midpoint, counted backwards, or `None` with fewer than two visible steps.
    /// # Returns
    /// The selected path element.
    pub fn midpoint(&self) -> Option<&Step> {
        let n = self.visible_steps();
        if n < 2 {
            return None;
        }
        // Native path_midpoint walks back from the end past n / 2 visible elements.
        self.steps.iter().rev().filter(|s| !s.hidden).nth(n / 2)
    }
    /// Estimate the bisect comparisons that remain, with the native ceiling of log2.
    /// # Returns
    /// Zero for zero or one visible step.
    pub fn search_depth(&self) -> u32 {
        let n = self.visible_steps();
        if n <= 1 {
            0
        } else {
            usize::BITS - (n - 1).leading_zeros()
        }
    }
}
/// An `Ancestor` holds the closest common ancestor and the native diagnostic path through
/// that pivot.
#[derive(Clone, Debug, PartialEq)]
pub struct Ancestor {
    /// Shared ancestor identifier.
    pub pivot: i32,
    /// Diagnostic path, whose native orientation can begin at either requested endpoint.
    pub steps: Vec<Step>,
}
/// An `Error` reports a failure of the graph, the input or a search.
///
/// A search that finds no path succeeds with `None`.
#[derive(Debug)]
pub enum Error {
    /// A finite input or search budget was exceeded.
    Limit,
    /// The input has a duplicate, nonpositive or unknown identifier (also in `hidden`) or bad
    /// metadata, or a search met a cyclic predecessor chain. Cyclic rows are accepted.
    InvalidGraph,
    /// A finite time difference or accumulated cost overflowed.
    NonFiniteCost,
    /// A checked allocation failed. The variant keeps the original cause of the reservation.
    Allocation(TryReserveError),
}
impl fmt::Display for Error {
    /// Format the bounded failure category.
    /// # Errors
    /// Returns the formatter error when writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "history graph error: {self:?}")
    }
}
impl std::error::Error for Error {
    /// Retain the typed cause when a checked reservation of a vector fails.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation(cause) => Some(cause),
            _ => None,
        }
    }
}

/// A `Graph` borrows metadata and indexes graph rows in their query order, with no storage
/// authority.
pub struct Graph<'a> {
    /// Borrowed resolved metadata.
    nodes: &'a [Checkin<'a>],
    /// Sorted map from each unique native ID to its metadata index.
    ///
    /// The sort does not reorder graph rows.
    ids: Vec<(i32, usize)>,
    /// Forward rows, retaining primary flags and relative query order.
    children: Vec<Vec<(usize, bool)>>,
    /// Backwards rows, retaining primary flags and relative query order.
    parents: Vec<Vec<(usize, bool)>>,
    /// Initial-hop rows, retaining exclusion flags.
    cherries: Vec<Vec<(usize, bool)>>,
    /// Validated caller bounds for each search.
    limits: Limits,
}
/// Append one element after a checked reservation, with no implicit infallible growth.
/// # Errors
/// Returns the allocation failure before the vector grows.
/// # Arguments
/// * `v` - Vector whose capacity must be checked before appending.
/// * `item` - Element to append after successful reservation.
/// # Returns
/// Unit after the element is appended.
fn push<T>(v: &mut Vec<T>, item: T) -> Result<(), Error> {
    reserve(v, 1)?;
    v.push(item);
    Ok(())
}
/// Reserve storage, and keep its original error of allocation or capacity.
/// # Errors
/// Returns the typed failure of the reservation, with its cause.
/// # Arguments
/// * `v` - Vector whose allocation must retain a typed failure.
/// * `additional` - Number of additional elements for which to reserve capacity.
/// # Returns
/// Unit once the requested spare capacity is available.
fn reserve<T>(v: &mut Vec<T>, additional: usize) -> Result<(), Error> {
    v.try_reserve(additional).map_err(Error::Allocation)
}
/// Allocate an indexed membership vector in one checked reservation.
/// # Errors
/// Returns the typed reservation failure before initializing elements.
/// # Arguments
/// * `len` - Number of graph indexes to represent.
/// # Returns
/// A checked allocation of false membership flags for every graph index.
fn membership(len: usize) -> Result<Vec<bool>, Error> {
    let mut flags = Vec::new();
    reserve(&mut flags, len)?;
    flags.resize(len, false);
    Ok(flags)
}
impl<'a> Graph<'a> {
    /// Validate explicit rows and preserve their relative order per query direction.
    /// # Arguments
    /// * `nodes` - Unique positive IDs with resolved metadata of branch and time.
    /// * `links` - Ordered native parent rows.
    /// * `cherries` - Ordered native cherrypick rows.
    /// * `limits` - Finite admission and per-search bounds.
    /// # Returns
    /// A graph independent of any database or authorization context.
    /// # Errors
    /// Rejects excessive inputs, missing endpoints, invalid metadata or allocation failure.
    pub fn new(
        nodes: &'a [Checkin<'a>],
        links: &[ParentLink],
        cherries: &[Cherrypick],
        limits: Limits,
    ) -> Result<Self, Error> {
        if nodes.len() > limits.checkins
            || links
                .len()
                .checked_add(cherries.len())
                .ok_or(Error::Limit)?
                > limits.links
        {
            return Err(Error::Limit);
        }
        let mut ids = Vec::new();
        reserve(&mut ids, nodes.len())?;
        let mut bytes = 0usize;
        for (i, n) in nodes.iter().enumerate() {
            bytes = bytes.checked_add(n.branch.len()).ok_or(Error::Limit)?;
            if bytes > limits.branch_bytes {
                return Err(Error::Limit);
            }
            if n.rid <= 0 || !n.mtime.is_finite() || n.branch.contains(&0) {
                return Err(Error::InvalidGraph);
            }
            ids.push((n.rid, i)); // Entire input length was fallibly reserved above.
        }
        ids.sort_unstable_by_key(|&(rid, _)| rid);
        if ids.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(Error::InvalidGraph);
        }
        let mut g = Self {
            nodes,
            ids,
            children: Vec::new(),
            parents: Vec::new(),
            cherries: Vec::new(),
            limits,
        };
        for _ in nodes {
            push(&mut g.children, Vec::new())?;
            push(&mut g.parents, Vec::new())?;
            push(&mut g.cherries, Vec::new())?;
        }
        for l in links {
            let p = g.id(l.parent)?;
            let c = g.id(l.child)?;
            push(&mut g.children[p], (c, l.primary))?;
            push(&mut g.parents[c], (p, l.primary))?;
        }
        for l in cherries {
            let p = g.id(l.parent)?;
            let c = g.id(l.child)?;
            push(&mut g.cherries[p], (c, l.excluded))?;
        }
        Ok(g)
    }
    /// Resolve a supplied record identifier.
    /// # Errors
    /// Returns `Error::InvalidGraph` for an unknown identifier.
    /// # Arguments
    /// * `rid` - Positive native record identifier to resolve in the admitted graph.
    /// # Returns
    /// The metadata index associated with that native identifier.
    fn id(&self, rid: i32) -> Result<usize, Error> {
        self.ids
            .binary_search_by_key(&rid, |&(id, _)| id)
            .map(|i| self.ids[i].1)
            .map_err(|_| Error::InvalidGraph)
    }
    /// Port `path_shortest`, including hidden-node accounting and first-hop cherrypicks.
    /// # Arguments
    /// * `from` - Starting check-in ID.
    /// * `to` - Destination check-in ID.
    /// * `options` - Native path flags and hidden-node set.
    /// # Returns
    /// Ordered path, or none if the native search finds no path.
    /// # Errors
    /// Returns input, finite-cost, allocation or search-budget failures.
    /// # Examples
    /// ```
    /// use gossil::history::{Checkin, Direction, Graph, Limits, Options, ParentLink};
    ///
    /// let nodes: Vec<_> = (1..=3)
    ///     .map(|rid| Checkin { rid, branch: b"trunk", mtime: 0.0 })
    ///     .collect();
    /// let links = [
    ///     ParentLink { parent: 1, child: 2, primary: true },
    ///     ParentLink { parent: 2, child: 3, primary: true },
    /// ];
    /// let limits = Limits {
    ///     checkins: 3,
    ///     links: 2,
    ///     branch_bytes: 15,
    ///     search_nodes: 16,
    ///     row_visits: 16,
    /// };
    /// let graph = Graph::new(&nodes, &links, &[], limits)?;
    /// let options = Options {
    ///     primary_only: false,
    ///     direction: Direction::Both,
    ///     hidden: &[],
    ///     branch_cost: 0,
    /// };
    /// let path = graph.shortest(3, 1, options)?.expect("a path");
    /// let rids: Vec<_> = path.steps.iter().map(|s| s.rid).collect();
    /// assert_eq!(rids, [3, 2, 1]);
    /// assert_eq!(path.cost, 3.0);
    /// # Ok::<(), gossil::history::Error>(())
    /// ```
    pub fn shortest(
        &self,
        from: i32,
        to: i32,
        options: Options<'_>,
    ) -> Result<Option<Path>, Error> {
        let start = self.id(from)?;
        let end = self.id(to)?;
        if options.branch_cost > i32::MAX as u32 || options.hidden.len() > self.limits.checkins {
            return Err(Error::Limit);
        }
        let mut hidden = membership(self.nodes.len())?;
        for rid in options.hidden {
            hidden[self.id(*rid)?] = true;
        }
        let mut s = Search::new(self, options.branch_cost);
        s.add(start, None, false, false, false)?;
        if start == end {
            return Ok(Some(s.path(0)?));
        }
        if options.direction == Direction::ForwardWithCherrypicks {
            for &(node, excluded) in &self.cherries[start] {
                s.visit()?;
                if !excluded {
                    s.add(node, Some(0), true, hidden[node], false)?;
                }
            }
        }
        let mut seen = membership(self.nodes.len())?;
        while let Some(idx) = s.pop() {
            let n = s.records[idx];
            if n.node == end {
                return Ok(Some(s.path(idx)?));
            }
            if seen[n.node] {
                continue;
            }
            seen[n.node] = true;
            // Native UNION ALL yields all children before any backwards rows.
            for &(node, primary) in &self.children[n.node] {
                s.visit()?;
                if (!options.primary_only || primary) && !seen[node] {
                    s.add(node, Some(idx), true, hidden[node], false)?;
                }
            }
            if options.direction == Direction::Both && !n.forward {
                for &(node, primary) in &self.parents[n.node] {
                    s.visit()?;
                    if (!options.primary_only || primary) && !seen[node] {
                        s.add(node, Some(idx), false, hidden[node], false)?;
                    }
                }
            }
        }
        Ok(None)
    }
    /// Port the two-front native `path_common_ancestor` search and path rewiring.
    /// # Arguments
    /// * `me` - First endpoint. Zero means no ancestor, as in native Fossil.
    /// * `you` - Second endpoint. Zero means no ancestor.
    /// # Returns
    /// Closest shared ancestor by native queue order, with its diagnostic path.
    /// Equal positive endpoints return the one-element path without stale global state.
    /// # Errors
    /// Returns input, allocation or search-budget failures.
    /// # Examples
    /// ```
    /// use gossil::history::{Checkin, Graph, Limits, ParentLink};
    ///
    /// let nodes: Vec<_> = (1..=3)
    ///     .map(|rid| Checkin { rid, branch: b"trunk", mtime: 0.0 })
    ///     .collect();
    /// // Check-ins 2 and 3 are both children of check-in 1.
    /// let links = [
    ///     ParentLink { parent: 1, child: 2, primary: true },
    ///     ParentLink { parent: 1, child: 3, primary: true },
    /// ];
    /// let limits = Limits {
    ///     checkins: 3,
    ///     links: 2,
    ///     branch_bytes: 15,
    ///     search_nodes: 16,
    ///     row_visits: 16,
    /// };
    /// let graph = Graph::new(&nodes, &links, &[], limits)?;
    /// let ancestor = graph.common_ancestor(2, 3)?.expect("an ancestor");
    /// assert_eq!(ancestor.pivot, 1);
    /// assert!(graph.common_ancestor(0, 3)?.is_none());
    /// # Ok::<(), gossil::history::Error>(())
    /// ```
    pub fn common_ancestor(&self, me: i32, you: i32) -> Result<Option<Ancestor>, Error> {
        if me == 0 || you == 0 {
            return Ok(None);
        }
        let a = self.id(me)?;
        let b = self.id(you)?;
        let mut s = Search::new(self, 0);
        s.add(a, None, false, false, true)?;
        if a == b {
            return Ok(Some(Ancestor {
                pivot: me,
                steps: s.path(0)?.steps,
            }));
        }
        s.add(b, None, false, false, false)?;
        let mut seen = [membership(self.nodes.len())?, membership(self.nodes.len())?];
        seen[0][b] = true;
        seen[1][a] = true;
        while let Some(idx) = s.pop() {
            let n = s.records[idx];
            let side = usize::from(n.side);
            for &(parent, _) in &self.parents[n.node] {
                s.visit()?;
                if seen[1 - side][parent] {
                    // pAll is newest-first, including records from either side.
                    let mut current = s
                        .records
                        .iter()
                        .rposition(|r| r.node == parent)
                        .ok_or(Error::InvalidGraph)?;
                    let mut previous = idx;
                    loop {
                        let next = s.records[current].from;
                        s.records[current].from = Some(previous);
                        previous = current;
                        if let Some(next) = next {
                            current = next;
                        } else {
                            break;
                        }
                    }
                    return Ok(Some(Ancestor {
                        pivot: self.nodes[parent].rid,
                        steps: s.path(previous)?.steps,
                    }));
                }
                if !seen[side][parent] {
                    seen[side][parent] = true;
                    s.add(parent, Some(idx), false, false, n.side)?;
                }
            }
        }
        Ok(None)
    }
}
/// An arena node with the fields of the native `PathNode` that the two algorithms use.
#[derive(Clone, Copy)]
struct Record {
    /// Metadata index of this pending record.
    node: usize,
    /// Arena index of the native predecessor.
    from: Option<usize>,
    /// Native `fromIsParent` direction bit.
    forward: bool,
    /// Native hidden-node bit.
    hidden: bool,
    /// Native common-ancestor primary-side bit.
    side: bool,
    /// Cumulative native queue priority.
    cost: f64,
}
/// Independent search state replaces Fossil's module-global state.
struct Search<'g, 'a> {
    /// Validated graph and bounds.
    graph: &'g Graph<'a>,
    /// The reverse order of the arena stands for the newest-first native `pAll` list.
    records: Vec<Record>,
    /// Native binary heap of arena indexes.
    heap: Vec<usize>,
    /// Native branch-change penalty, zero for edge costs.
    branch_cost: u32,
    /// Charged adjacency-row work.
    visits: usize,
}
impl<'g, 'a> Search<'g, 'a> {
    /// Start an empty bounded search.
    /// # Arguments
    /// * `graph` - Validated graph that supplies metadata and finite search bounds.
    /// * `branch_cost` - Native penalty for crossing branches, or zero for edge-count cost.
    /// # Returns
    /// Empty search state that borrows the graph and owns its queue and record arena.
    fn new(graph: &'g Graph<'a>, branch_cost: u32) -> Self {
        Self {
            graph,
            records: Vec::new(),
            heap: Vec::new(),
            branch_cost,
            visits: 0,
        }
    }
    /// Charge one adjacency row.
    /// # Errors
    /// Returns limit exhaustion before further traversal.
    /// # Returns
    /// Unit when one more adjacency-row visit fits the search budget.
    fn visit(&mut self) -> Result<(), Error> {
        self.visits = self.visits.checked_add(1).ok_or(Error::Limit)?;
        if self.visits > self.graph.limits.row_visits {
            Err(Error::Limit)
        } else {
            Ok(())
        }
    }
    /// Port `path_new_node` and the strict comparisons of `pqueuex_rebalance`.
    /// # Errors
    /// Returns arena limits, allocation failure or nonfinite cumulative costs.
    /// # Arguments
    /// * `node` - Graph metadata index for the queued candidate.
    /// * `from` - Predecessor arena index, or none for an initial candidate.
    /// * `forward` - Whether the candidate was reached in the parent-to-child direction.
    /// * `hidden` - Whether this candidate is hidden in the returned path.
    /// * `side` - Which endpoint frontier owns the candidate in the ancestor search.
    /// # Returns
    /// Unit after recording the candidate and restoring native heap priority.
    fn add(
        &mut self,
        node: usize,
        from: Option<usize>,
        forward: bool,
        hidden: bool,
        side: bool,
    ) -> Result<(), Error> {
        if self.records.len() >= self.graph.limits.search_nodes {
            return Err(Error::Limit);
        }
        let mut cost = from.map_or(0.0, |i| self.records[i].cost);
        if self.branch_cost == 0 {
            cost += 1.0;
        } else if let Some(i) = from {
            let a = self.graph.nodes[self.records[i].node];
            let b = self.graph.nodes[node];
            cost += (a.mtime - b.mtime).abs();
            if a.branch != b.branch {
                cost += f64::from(self.branch_cost);
            }
        }
        if !cost.is_finite() {
            return Err(Error::NonFiniteCost);
        }
        let idx = self.records.len();
        let record = Record {
            node,
            from,
            forward,
            hidden,
            side,
            cost,
        };
        push(&mut self.records, record)?;
        push(&mut self.heap, idx)?;
        let mut i = self.heap.len() - 1;
        while i > 0 {
            let j = (i - 1) / 2;
            if self.records[self.heap[j]].cost <= self.records[self.heap[i]].cost {
                break;
            }
            self.heap.swap(i, j);
            i = j;
        }
        Ok(())
    }
    /// Port `pqueuex_pop`, which swaps through equal costs and prefers the left child on a tie.
    /// # Returns
    /// The next arena index in native priority order, or none for an empty queue.
    fn pop(&mut self) -> Option<usize> {
        let result = *self.heap.first()?;
        let last = self.heap.pop()?;
        if !self.heap.is_empty() {
            self.heap[0] = last;
            let mut i = 0usize;
            while let Some(mut j) = i
                .checked_mul(2)
                .and_then(|v| v.checked_add(1))
                .filter(|j| *j < self.heap.len())
            {
                if j + 1 < self.heap.len()
                    && self.records[self.heap[j]].cost > self.records[self.heap[j + 1]].cost
                {
                    j += 1;
                }
                if self.records[self.heap[i]].cost < self.records[self.heap[j]].cost {
                    break;
                }
                self.heap.swap(i, j);
                i = j;
            }
        }
        Some(result)
    }
    /// Reverse the predecessor chain into an owned path.
    /// # Errors
    /// Returns allocation failure or an internally cyclic predecessor chain.
    /// # Arguments
    /// * `end` - Arena index whose predecessor chain ends the requested path.
    /// # Returns
    /// The ordered native steps and cumulative cost from the initial record to this endpoint.
    fn path(&self, end: usize) -> Result<Path, Error> {
        let mut steps = Vec::new();
        let mut at = Some(end);
        while let Some(i) = at {
            if steps.len() >= self.records.len() {
                return Err(Error::InvalidGraph);
            }
            let r = self.records[i];
            let step = Step {
                rid: self.graph.nodes[r.node].rid,
                from_is_parent: r.forward,
                hidden: r.hidden,
            };
            push(&mut steps, step)?;
            at = r.from;
        }
        steps.reverse();
        Ok(Path {
            steps,
            cost: self.records[end].cost,
        })
    }
}

#[cfg(test)]
mod tests {
    //! Allocation and heap controls exercise private seams without large allocations.
    use super::*;
    use std::error::Error as _;

    /// Capacity overflow takes the production reservation error path before allocation.
    #[test]
    fn reservation_capacity_overflow_retains_typed_cause() {
        let mut bytes = Vec::<u8>::new();
        let error = reserve(&mut bytes, usize::MAX).unwrap_err();
        assert!(matches!(&error, Error::Allocation(_)));
        assert!(
            error
                .source()
                .unwrap()
                .downcast_ref::<TryReserveError>()
                .is_some()
        );
        assert!(bytes.is_empty());
        assert_eq!(bytes.capacity(), 0);
        assert!(Error::Limit.source().is_none());
    }

    /// Equal priorities swap down on pop, choose the left child, and never sift up.
    #[test]
    fn equal_cost_heap_order_matches_native_pop_and_rebalance() {
        let nodes: Vec<_> = (1..=5)
            .map(|rid| Checkin {
                rid,
                branch: b"trunk",
                mtime: 0.0,
            })
            .collect();
        let graph = Graph::new(
            &nodes,
            &[],
            &[],
            Limits {
                checkins: 5,
                links: 0,
                branch_bytes: 25,
                search_nodes: 5,
                row_visits: 0,
            },
        )
        .unwrap();
        let mut search = Search::new(&graph, 0);
        for node in 0..5 {
            search.add(node, None, false, false, false).unwrap();
        }
        // Not FIFO: native replacement swaps through equal left children.
        let order: Vec<_> = std::iter::from_fn(|| search.pop()).collect();
        let capture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/history/native-queue.json"))
                .unwrap();
        let native_order: Vec<_> = capture["pops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(native_order, [0, 1, 3, 4, 2]);
        assert_eq!(order, native_order);
    }

    /// A predecessor chain that loops gives an error and not an endless walk.
    #[test]
    fn cyclic_predecessor_chain_is_rejected() {
        let nodes = [
            Checkin {
                rid: 1,
                branch: b"trunk",
                mtime: 0.0,
            },
            Checkin {
                rid: 2,
                branch: b"trunk",
                mtime: 0.0,
            },
        ];
        let graph = Graph::new(
            &nodes,
            &[],
            &[],
            Limits {
                checkins: 2,
                links: 0,
                branch_bytes: 10,
                search_nodes: 2,
                row_visits: 0,
            },
        )
        .unwrap();
        let mut search = Search::new(&graph, 0);
        search.add(0, None, false, false, false).unwrap();
        search.add(1, Some(0), true, false, false).unwrap();
        assert_eq!(search.path(1).unwrap().steps.len(), 2);
        search.records[0].from = Some(1);
        assert!(matches!(search.path(1), Err(Error::InvalidGraph)));
    }
}
