# Automatic face release validation

Code tags now prepare both code and native face artifacts before publication.
The code release publishes first; a dependent job creates and publishes the face
release in the same workflow. Token-created tags therefore do not need to trigger
another workflow. Explicit `faces-*` tags remain available for data-only releases.

The default source is the original Scryfall Oracle snapshot from 2026-11-12
22:01:58 UTC, checksum-pinned in `releases/oracle-source.json`. It contains 39,288
records and 42,679 faces. Gzip JSON Lines are decoded without Oracle text rewrites.
The original compressed bytes are included in the verified release assets.

The actual `faces-code` command succeeded against `grammar-v0.1.0`, building the
released code commit `0edb7f52a83b1fae15ddd369aad33a7a5c18a763` in a temporary
worktree while exporter commit `b56cff8c989748d2a684741033608495dec2a213` supplied
packaging. This exercises backfill of the original release despite newer,
unreleased grammar changes on main. The isolated build cache and expanded
intermediate files were removed after verification.

The bundle has 24,725 unique faces, 984 ambiguous faces, 16,970 refusals and no
limits. All outcomes and distinct ambiguous readings are retained. Reference
agreement, printing/reparse and exact wording are explicitly **not run**; this
release check does not claim those gates passed. See the
[artifact proof](automatic-face-release-20261113.proof.json) for checksums.

## Publishing worker handoff

No release tag or GitHub release was created during this validation. The first
face bundle can be prepared from main without republishing any crates:

```sh
gh workflow run release-faces.yml --repo vishvananda/oracle-grammar --ref main -f grammar_tag=grammar-v0.1.0 -f publish=false
```

The workflow's `native-faces` artifact contains the complete bundle. The publishing
worker can select `publish=true` to create the verified GitHub release and tag.
Alternatively use the explicit checked-in
`releases/faces/faces-oracle-20261112220158-s190-g0.1.0-r1.json` specification.
The source is packaged with the bundle, so consumers can still verify its hash
if the original provider URL later expires. New code tags use this route
automatically; changing the default snapshot requires an explicit pin update.

CI uses `CRATES_IO_PUBLISH_TOKEN`, the installed update-only secret, with OIDC as
the fallback. Initial crate registration remains local. A separate preexisting
CI failure was reproduced: standalone library tests passed, then receipt writing
failed because `.cache/dev` had never been created. Receipt writing now creates
its parent directory after success, with a fresh-cache regression test.

MTGish fork commit `1fb1dec3b101bbefbe55870c108739709a04e53e` adds gzip JSON Lines
input to the bundle comparison gate while retaining legacy JSON arrays. Its six
Python comparison tests pass. AST layouts and model versions are unchanged.

## Validation

`python3 tools/dev.py check` passed formatting, pins, hygiene, Clippy, helper
builds, all 27 compositional families (447 cases), and workspace library tests
(including 677 grammar tests, one intentionally ignored). After the fresh-cache
receipt fix, all 70 Python tool tests passed. Actionlint 1.7.7 passed both release
workflows. The actual `faces-code` export and independent artifact checksum
verification passed. No Rust model, schema or grammar behavior changed here.
