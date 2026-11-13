# MTGish validation boundary

The MTGish fork supplies reference card semantics. Its reference revision is
independent of the schema implementation currently used by the frontend.

- `../../pins/model.json` identifies the owned model packages and extraction origin.
- `reference.json` independently pins validation data and its producer.
- `../../src/model.rs` imports the owned workspace model, not MTGish code.

The current reference and owned model share schema 190, so decoding current
native JSON needs no schema translation. Older or divergent reference schemas
must be handled explicitly; unmapped constructs must fail. The adapter is a
data boundary, not a dependency of the owned model crates.

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

## IR ownership

Ownership has moved to `../../crates/oracle-ir` and
`../../crates/oracle-ir-contract`. The initial extraction preserves the current
wire format and contracts. Historical wire readers and authoring compatibility
helpers are retained, but neither the legacy Source compiler nor a Cantrip
backend is a frontend dependency. See
[the migration record](../../docs/owned-ir-crates.md).

The MTGish fork can depend on these packages directly at an immutable commit.
It should re-export shared types rather than keep a second local definition.
Reference snapshots remain independently pinned artifacts. Advancing a reference
revision must not change the owned model or rebuild ordinary grammar helpers.
