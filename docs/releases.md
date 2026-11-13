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
python3 tools/releases.py github .cache/release-code --repo OWNER/REPO --publish
```

Publication proceeds `oracle-ir`, `oracle-ir-contract`, then `oracle-ir-grammar`.
A dry run must reproduce the reviewed `.crate` checksum before each upload.
Cargo 1.98 stages `publish --dry-run` candidates under
`target/package/tmp-crate/`; the checksum check reads that fresh candidate, not
a previous `cargo package` archive. If the publisher needs repair after a code
tag is prepared, `publish-crates --root /path/to/clean/tag-checkout DIRECTORY`
can use that exact immutable producer with the repaired tool. Its commit and
grammar-input identity must match the reviewed manifest. The tag and prepared
assets are not moved or replaced.

Unchanged published model versions are reused as their exact registry archives.
Their source and dependency declarations must match the current checkout;
regenerated Cargo provenance, manifest formatting and library lockfiles do not force a
model version bump. A changed model source requires a new model version. Only
new prerequisite versions enter Cargo's temporary registry, so dependent package
locks retain the real checksums of reused dependencies. Already published grammar
versions must match their exact release checksum.
GitHub uploads go to a draft, are downloaded and verified, then published.
An interrupted existing GitHub draft requires inspection rather than replacement.

After initial local registration, CI uses the scoped, update-only repository
secret `CRATES_IO_PUBLISH_TOKEN`. If that secret is absent, configure each crate's
Trusted Publisher for this repository, `release-grammar.yml`, and environment
`crates-io`; CI then obtains a short-lived OIDC token. The broad bootstrap token
is not used or required by CI. Pushing a `grammar-v*` tag publishes after all
checks and matching native face preparation pass. Manual dispatch defaults to
preparation only. The code release is followed by its face tag/release in the
same workflow, avoiding GitHub's suppression of token-generated push events.
Enable GitHub release immutability in repository settings for immutable tags and
assets. Cargo registry publication is permanent.

## Native face exports

The provider adapter reads **original Scryfall Oracle bulk JSON**, either an
array or gzip-compressed JSON Lines (`encoding: "jsonl-gzip"`). It
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

Code releases automatically use the SHA-pinned `releases/oracle-source.json`.
The source snapshot is fixed until that file is deliberately updated. Each
bundle retains the original source asset with its original checksum.

For an independent data release, commit a source/population specification under
`releases/faces/<TAG>.json`, then push the corresponding `faces-*` tag. The
**Release native faces** workflow downloads and verifies that source, requires
the exporter to build the exact existing grammar tag in an isolated worktree, and publishes the
verified GitHub bundle. See [the tag specification](../releases/README.md).
Manual dispatch defaults to artifact preparation only. Data releases do not
require a crates.io release or token. Export revision increments are for
packaging changes, never hidden grammar changes.

For an existing code release whose original workflow had no automatic face job,
dispatch **Release native faces** on main with `grammar_tag=grammar-v0.1.0`
and no face `tag`. This uses the default source pin with that exact old grammar;
it does not republish crates. Use `publish=false` to inspect CI artifacts first.
