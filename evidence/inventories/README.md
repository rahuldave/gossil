# Source and test disposition inventories

These archives map every tracked Fossil and Gest file to a proposed destination,
required contracts, related tests and unresolved acceptance work. They record
observations and proposed port ownership. They do not implement those contracts
or establish passing compatibility tests.

## Captured surfaces

| Family | Complete mapped surface |
| --- | --- |
| Fossil | 984 tracked files, including 229 source, 81 test and 78 tools files |
| Fossil diagnostics | 161 source declarations and 158 available reference commands |
| Fossil suites | All 37 Tcl suites, with existing classified baseline outcomes |
| Gest files | 544 tracked entries, comprising 541 regular files and three symlinks |
| Gest tests | 1,214 compiled identities, comprising 867 unit and 347 integration tests |
| Gest CLI | 93 command nodes, 72 JSON nodes and 29 alias declarations |
| Gest aliases | 27 command aliases and two option aliases |
| Gest HTTP and storage | 34 routes, 15 migrations and 15 derived tables |

The PORT-02 acceptance requirement defines three dispositions, or decisions
about how to carry each surface into Gossil. They are `port` for ported behavior,
`replace-with-equivalent` for equivalent replacement behavior and
`supporting-source` for supporting materials. Fossil has 110, 95 and 779
files in those categories. Effective mappings combine the archived file and
family rows with the correction overlay described below.
Supporting sources retain their recorded build, fixture, documentation or
dependency role. That category does not remove engine mechanisms from scope.

The archived Fossil `consistency.json` records 20 historical identity and mapping checks.
Gest's `validation.json` records complete file, test and API-set checks.
The executed inventory checks cover identity and mapping. Behavior acceptance
remains pending.
Semantic coverage, missing build configurations and storage encodings remain
required work. Known baseline failures and timeouts remain unchanged.

## Current correction and provenance layer

Read the archived Fossil inventory together with
[family corrections](fossil/family-corrections.json). Apply its fourteen family
corrections over the archived fields; retain all other fields.
The validator combines them in memory for checks and does not write a merged inventory.

The effective family dispositions use the same three values as file rows.
The validator rejects extra fields in each family correction, so the overlay
cannot replace archived contracts, destinations or follow-up work.
Adaptation actions and pending work have separate fields. Windows remains
required port scope, with qualification pending.

The [pinned Git tree manifest](fossil/pinned-git-tree.json) adds file kinds,
Git modes and blob identities for all 984 Fossil paths. Its
[raw tree capture](fossil/pinned-git-tree.ls-tree) comes from the exact pinned
Git mirror revision. There are 960 regular files with mode `100644` and
24 executable files with mode `100755`; this tree contains no symlinks.
The validator handles symlink blobs as link text when present.
These Git identities are separate from the recorded native Fossil manifest ID.
The capture does not reconstruct or verify a native Fossil manifest.

The [Fossil relocation index](fossil/relocation-index.json) lists ten archived originals,
and the [Gest relocation index](gest/relocation-index.json) lists thirteen.
The [current validator](validate_current.py) checks all 23 archived originals,
every effective disposition field and complete path, mode, kind and blob sets.
It compares manifest hashes to `fossil/inventory.json.gz` and
`fossil/source-inventory-input.json`, the two archived Fossil byte inventories.
Run it from the repository root:

```sh
python3 evidence/inventories/validate_current.py
python3 evidence/inventories/validate_current.py --git-source /absolute/path/to/pinned-fossil-git-checkout
```

The second command also checks the actual Git tree and all 984 blob objects.
The [executed validation receipt](correction-validation.json) records that check.
The [correction receipt](correction-receipt.json) identifies companion hashes
and preserved original identities. Its validator and test hashes identify the
correction checkpoint before later docstring edits. The final gate must cover
those documentation changes.

Fresh independent review must cover this
correction layer before the final checkpoint.
These checks establish planning identities and mappings. Semantic coverage,
platform configurations and engine acceptance remain open.

## Exact bytes and relocation

Each family has a `relocation-index.json` mapping original paths to captured
artifacts, byte lengths and SHA-256 hashes. `inventory.json.gz` preserves the
exact original JSON bytes under gzip compression. The index hash covers the
uncompressed bytes. Original reports, checker results, input manifests and
generation scripts retain their original contents and provenance.

The archived generation and measurement scripts are historical evidence,
separate from the current validator and its regression tests.
They retain absolute paths to pinned source trees, specifications, scratch
inputs and compiled test binaries. Reproduction requires restoring those
inputs and decompressing the inventory. Relocation does not turn those paths
into installed runtime dependencies or prove a portable acquisition procedure.

The prior Gest baseline followed symlinks when hashing document contents.
The new inventory distinguishes pinned link text, Git modes and blob hashes
from resolved-content hashes. Its crosswalk preserves both meanings. Earlier
receipts were not rewritten to make the two hash conventions appear equal.

No delta corpus or engine implementation is included in this capture. Related
scratch references in the original inventories remain historical references.

## Recommendations

Use these complete mapped sets for independent review and incremental port work.
Next, resolve the named contract and configuration gaps while keeping inherited
assertions and explicit replacement mappings intact.
