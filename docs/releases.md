# Releasing Oracle Grammar

The grammar uses Cargo SemVer. Its output schema and the Oracle source snapshot
are independent identities. Code tags are `grammar-v<VERSION>`; data tags are
`faces-oracle-<SNAPSHOT>-s<SCHEMA>-g<VERSION>-r<REVISION>`. The tag, producer commit,
grammar-input hashes, package versions and asset hashes are recorded in each
manifest. Published assets are never overwritten. See
[the versioning design](grammar-release-plan.md) for the reasoning.

## Code and crates.io

Work from a clean, reviewed commit. Update the package versions and matching local
dependency versions when their contents change; update `pins/model.json` and
`Cargo.lock` together. Run the repository's normal CI checks before publication.

```sh
python3 tools/releases.py code --tag grammar-v0.1.0 --out .cache/release-code
python3 tools/releases.py verify .cache/release-code
```

This verifies all three actual Cargo packages, builds the portable export tools,
runs compositional family gates and a synthetic native-CLI smoke test,
and creates the `.crate` files, modular grammar archive, flattened EBNF,
`manifest.json` and `SHA256SUMS`. The model dependencies are packaged with the
grammar release so their precise bytes are independently inspectable. Full
corpora and reference data are excluded. `embedded-grammar` embeds syntax for
portable binaries. Without that feature development retains hot-loading;
`ORACLE_GRAMMAR_DIR` still overrides either mode explicitly. Release tooling
rejects external grammar and compiler-flag overrides.

For the first registry publication, authenticate with `cargo login` locally:

```sh
python3 tools/releases.py publish-crates .cache/release-code
git tag grammar-v0.1.0
git push origin grammar-v0.1.0
```

Initial crate registration uses the local Cargo credential. CI's
`CRATES_IO_PUBLISH_TOKEN` is scoped to publishing updates and cannot create new
crates. Tag the exact clean commit used for the local publication so CI can
verify and reuse those identical archives before publishing the GitHub assets.

Publication proceeds `oracle-ir`, `oracle-ir-contract`, then `oracle-ir-grammar`.
A dry run must reproduce the reviewed `.crate` checksum before each upload.
Unchanged published model versions are reused as their exact registry archives.
Their source and dependency declarations must match the current checkout;
regenerated Cargo provenance, manifest formatting and library lockfiles do not force a
model version bump. A changed model source requires a new model version. Only
new prerequisite versions enter Cargo's temporary registry, so dependent package
locks retain the real checksums of reused dependencies. Already published grammar
versions must match their exact release checksum.
GitHub uploads go to a draft, are downloaded and verified, then published.
An interrupted existing GitHub draft requires inspection rather than replacement.

After bootstrap configure each crate's crates.io Trusted Publisher with this
repository, `release-grammar.yml`, and environment `crates-io`. Configure that
GitHub environment's access policy. Pushing a `grammar-v*` tag runs **Release grammar** and publishes after all gates
pass. Manual dispatch defaults to preparation only. CI uses the update-only
repository secret `CRATES_IO_PUBLISH_TOKEN`. Once all three Trusted Publishers
are configured, remove that secret to switch to short-lived OIDC tokens.
Enable GitHub release immutability in repository settings for immutable tags and
assets. Cargo registry publication is permanent.

## Native face exports

The initial provider adapter reads **original Scryfall Oracle bulk JSON**. It
converts source characteristics separately from rules and never reads MTGish
reference IR. Unrepresentable source metadata (for example symbolic printed P/T)
is an explicit per-face `unsupported_source` outcome. This first adapter does
not promise full source-characteristic coverage; the reports retain that gap.
Layouts and provider/face identity are preserved around the individual typed
faces. These are face exports, not complete executable multi-face card records.

Create a source manifest from the actual immutable input, with a genuine provider
snapshot ID or content-hash ID. Do not label the local export time as the Oracle
source version. For example, substitute real identity and checksum values into:

```json
{
  "format_version": 1,
  "provider": "scryfall",
  "snapshot_id": "sha256-REPLACE_WITH_ACTUAL_INPUT_HASH",
  "sha256": "REPLACE_WITH_ACTUAL_64_HEX_SHA256",
  "population": {
    "kind": "all",
    "policy": "Explicitly authorized complete Oracle bulk population"
  }
}
```

An allowed subset uses `"kind":"oracle_ids"` and an `"oracle_ids":[...]` list
instead of `all`. Missing and duplicate requested IDs fail the export. Respect
the repository's held-out policy: use synthetic fixtures while authoring and an
explicitly authorized population for release exports.

For a local export, check out the immutable code-release tag, then run:

```sh
python3 tools/releases.py faces --source oracle.json --source-manifest source.json --out .cache/release-faces
python3 tools/releases.py verify .cache/release-faces
python3 tools/releases.py github .cache/release-faces --repo OWNER/REPO --publish
```

Every source occurrence gets one outcome, identified by provider ID, record
position within the selected input, layout and face position. Distinct meanings
are never chosen by first match. `outcomes.jsonl.gz` preserves all readings for
ambiguous inputs and explicit refusals/limits. `faces.jsonl.gz` contains only
unique, fully parsed, typed-validated native faces and their presentation notes
and source context. The wrapper verifies coverage counts and distinct reading
counts; deterministic ordering, JSON and gzip headers give reproducible assets.
Printing, semantic reparse, exact wording and reference agreement are labeled
`not_run`; parsing success does not imply those additional gates.

For CI, commit a SHA-pinned source/population specification under
`releases/faces/<TAG>.json`, then push the corresponding `faces-*` tag. The
**Release native faces** workflow downloads and verifies that source, requires
compiled grammar inputs to match the existing code tag, and publishes the
verified GitHub bundle. See [the tag specification](../releases/README.md).
Manual dispatch defaults to artifact preparation only. Data releases do not
require a crates.io release or token. Export revision increments are for
packaging changes, never hidden grammar changes.
