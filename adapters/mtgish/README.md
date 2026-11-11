# MTGish validation boundary

The MTGish fork supplies reference card semantics. Its reference revision is
independent of the schema implementation currently used by the frontend.

- `../../pins/model.json` and `../../Cargo.lock` pin the compiled model provider.
- `reference.json` pins optional validation data and the producer that emitted it.
- `../../src/model.rs` is the frontend's single model import boundary.

The current reference uses schema 189 and can be validated against the current
model without translation. A local IR schema and a Rust upstream-to-local
translator have not yet been extracted. Keeping this explicit avoids treating
reference corrections as schema migrations.

## Artifacts

`source` identifies the published MTGish Source RON input by immutable URL and
SHA-256. `normalized` identifies the complete native JSONL export, before any
local population selection or canonicalization. Its producer revision and byte
checksum are pinned. There is no published release download for that normalized
artifact yet; a supplied producer output can be verified directly.

```sh
python3 tools/pin.py check
python3 tools/pin.py fetch --kind source
python3 tools/pin.py verify /path/to/unified.jsonl
```

These commands run from the repository root. Downloads go to ignored `.cache/`.
They do not happen during ordinary builds or tests. Verifying a checksum proves
artifact identity; semantic validation and allowed-population selection remain
separate audit steps. Fetching an artifact does not authorize reading held-out
cards while writing grammar rules.

When releases are available, add a normalized artifact's immutable URL to its
lock entry, preserving the SHA-256 and producer revision. Prefer a release
bundle containing native data, schema/version metadata and checksums over a
submodule checkout or a platform-specific producer executable.

## Moving IR ownership here

1. Extract the schema, canonicalization, validation and binding contracts into a
   local crate, preserving existing fixtures and wire semantics first. Today
   those pieces span MTGish's IR and normalizer crates; changing only a dependency
   name does not separate them.
2. Route `src/model.rs` to the local crate. The parser and printer continue to
   consume the same local model boundary.
3. Put MTGish decoding and translation in an optional adapter crate or feature.
   Normal parsing, printing and tests then build without MTGish source packages.
4. Let the adapter support upstream schema revisions explicitly. Reject unmapped
   constructs; never discard meaning to improve an agreement count.
5. Publish validation snapshots from the owned fork, with reproducible producer
   instructions and artifact checksums. Reference fixes advance this lock without
   changing the local IR version or recompiling the grammar.

Changes in the fork are available when needed for that split or artifact
publication. They should preserve a buildable upstream reference and avoid
coupling its release schedule to the grammar's normal edit/test loop.
