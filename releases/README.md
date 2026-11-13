# Tag-driven releases

Pushing `grammar-v<VERSION>` runs **Release grammar**, verifies all packages and
publishes them to crates.io in dependency order, followed by a verified GitHub
code release. The tag must exactly match the root Cargo package version.

For a data release, commit `releases/faces/<TAG>.json`, then push that exact tag.
Its format is:

```json
{
  "format_version": 1,
  "grammar_tag": "grammar-v0.1.0",
  "revision": 1,
  "source": {
    "format_version": 1,
    "provider": "scryfall",
    "url": "https://example.invalid/immutable-oracle.json",
    "snapshot_id": "sha256-INPUT_HASH",
    "sha256": "REPLACE_WITH_ACTUAL_64_HEX_SHA256",
    "population": {
      "kind": "all",
      "policy": "Explicitly authorized complete Oracle bulk population"
    }
  }
}
```

The filename/tag must be
`faces-oracle-<snapshot_id>-s<schema>-g<grammar-version>-r<revision>`. The source
checksum and explicit population are mandatory. Grammar/model/compiled-source
inputs must match the existing `grammar_tag`; packaging-only changes may use a
new exporter commit. No placeholder specification is a real release.

Both workflows also support manual preparation without publishing. Never move
published tags. Register each crate initially from a local Cargo login before pushing its first
code tag. CI uses the update-only repository secret `CRATES_IO_PUBLISH_TOKEN`;
it cannot create new crates. Remove the secret after configuring Trusted
Publishing for all three crates with `release-grammar.yml` and GitHub environment
`crates-io`; later tag releases then use OIDC. See [the complete runbook](../docs/releases.md).
