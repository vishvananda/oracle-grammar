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

## Export updates

`updates.json` records the only local changes to reference records. The pinned
export stays byte-identical to its checksum; `tools/corpus.py prepare` applies
the updates while filtering it to the allowed population, and writes a receipt
of the faces each update changed. Every score prints those faces' statuses.

Each update has an id (`U0001`), a kind (`correction` when the reference meaning
is wrong, `normalization` when one wording has more than one encoding), a summary,
the evidence, and its changes. A change names a face, a path of keys and indexes
inside its ability list, the value expected there and the replacement. The record
names the reference revision it was written against.

Preparation fails when a named face is missing or repeated, when a path no longer
holds the recorded value, or when it already holds the replacement because
upstream adopted it. After advancing `reference.json`, drop adopted changes,
re-derive stale paths from the new export, and set the record's revision. List
each update under "Updated locally" in `reference-issues.md`, and report it
upstream so the reference can adopt it.

```sh
python3 tools/updates.py   # validate the record against the pinned reference
```

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
