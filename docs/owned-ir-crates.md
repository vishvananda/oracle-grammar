# Owned IR crates

Oracle Grammar owns the rules AST and shared contracts in two independent Rust
packages. The frontend, future MTGish lowering, backend generators and editing
tools can use exactly the same Rust types without a runtime conversion layer.

| Package | Owns |
| --- | --- |
| `oracle-ir` | Typed AST, closed game vocabulary, binary/JSON serialization, declaration registry, generated visitors and frozen wire schemas |
| `oracle-ir-contract` | Structural validation, binding/receipt semantics, canonicalization, compact card code, and retained historical input compatibility |
| `oracle-ir-grammar` | Oracle grammar, context binding, rendering and reference comparison |

The extraction source is MTGish fork commit
`54abf45e9992e52a49f7569f1d038b393a0fa9b1`. Imported code retains the MTGish MIT
license and copyright in each package. `pins/model.json` records this provenance;
it does not freeze owned code against future reviewed edits. The reference pin
is unchanged. No corpus or machine-specific dependency path is bundled.

## Initial compatibility boundary

The extraction preserves schema 190, enum order, numeric domains and AST payload
shape. Closed vocabularies are local definitions instead of a dependency on the
large legacy parser type package. The existing `mtg_types` import name is a
compatibility re-export of owned vocabulary, not a second set of types.

Portable validation and binding helpers have been separated from Source
compilation. Some compatibility helpers still decode historical `Operand`
recipes, entry payloads, keyword signatures and atomic-form names. These retain
existing validation/authoring behavior; they do not parse Oracle text, compile
legacy Source or call an engine. The package contains no MTGish dependency.

Large validator and semantic modules are split by responsibility. Large enum
matches retain arm order and bodies in bounded helpers, with inlined borrowed
dispatch. They do not serialize operands or allocate an adapter AST. The
ownership change does not add a schema migration or broaden canonical equality.

The declaration registry remains the single input for traversal and schema
metadata. Frozen input descriptors support historical readers. Edited compact
card code still ends in strict validation; names and source-site maps remain
outside semantic identity.

## Build and release behavior

Normal builds, tests, and diagnostics use workspace packages. There are no Git
model dependencies, submodules, patches or MTGish Source package downloads.
`tools/dev.py` fingerprints local crate code, manifests and embedded assets.
Changing these rebuilds helpers; changing runtime syntax or reference metadata
still does not. CI tests and lints all workspace packages.

Downstream consumers should depend on both packages at the same immutable Git
revision, using `default-features = false` if desired, then re-export their
existing public module paths while migrating. The grammar package itself is not
a backend dependency. Updating the MTGish fork to consume these crates is a
separate change after this ownership PR is reviewed and published.

Model changes start here with schema/semantic tests and consumer impact notes.
Wire changes update the schema and compatibility policy; implementation-only
moves do not. Reference corrections keep their independent artifact revision and
checksums. A future divergent reference schema needs an explicit adapter, never
a silent coercion or scorer alias.

## Validation

The PR retains the frozen schema tests and canonicalization regressions and runs
the frontend tests against the owned crates. Additional extraction and corpus
parity evidence is recorded with the final PR validation results. Full corpora
stay outside the repository and are not required for ordinary builds or tests.
