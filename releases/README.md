# Tag-driven releases

Pushing `grammar-v<VERSION>` runs **Release grammar**. CI verifies and packages
the crates and prepares the matching native face bundle **before publishing**.
It publishes the crates and code release first, then creates and publishes the
corresponding face tag and GitHub release in the same workflow. This does not
rely on a second workflow being triggered by `GITHUB_TOKEN`'s tag creation.

`releases/oracle-source.json` pins the default original Scryfall source URL,
SHA-256, encoding, snapshot identity, export revision and allowed population.
Update this pin deliberately when adopting a new source snapshot. Code releases
reuse it; they never silently download whichever bulk snapshot is newest.
The first pin is Scryfall's 2026-11-12 22:01:58 UTC snapshot, all 39,288 records.
Original source bytes are included in each bundle so expiring provider URLs do
not prevent downstream validation.

For a data-only release, commit `releases/faces/<TAG>.json`, then push that exact
tag. It has the same fields as the default source specification, plus
`"grammar_tag": "grammar-v0.1.0"`. The checked-in 0.1.0 face specification is a
complete example. The tag must be
`faces-oracle-<snapshot_id>-s<schema>-g<grammar-version>-r<revision>`.
Change the snapshot ID for source changes; increment the revision for exporter
changes. Grammar changes require a new grammar crate version.

Both routes build the exact immutable grammar tag in a temporary worktree,
independently of the exporter checkout. Thus a packaging fix can export an older
released grammar without accidentally using newer parser code. The manifest
records both producer and exporter commits and all input hashes. Missing pins,
checksum mismatches, absent requested IDs and incomplete outcome coverage fail.

Both workflows support preparation without publishing. To backfill an existing
code release, dispatch **Release native faces** on main with
`grammar_tag=grammar-v0.1.0`, leave `tag` empty, and choose whether to publish.
For an explicit data specification, supply `tag` and leave `grammar_tag` empty.
Never move published tags or overwrite published assets.

Initial crate registration is done locally. CI uses the update-only repository
secret `CRATES_IO_PUBLISH_TOKEN`, or short-lived OIDC when no token is configured
and all three crates have Trusted Publishing set up for `release-grammar.yml`
and environment `crates-io`. Faces are GitHub assets, never Cargo packages. See
[the complete runbook](../docs/releases.md).
