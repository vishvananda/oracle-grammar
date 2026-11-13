# Publishing the grammar and its native face exports

This proposal concerns `oracle-ir-grammar`: the parser, binder, Oracle printer
and the bundled grammar rules, plus the native faces produced by that grammar.
The shared model crates are prerequisites, not the release identity of this
product. No versions, release tags or registry contents are changed by this plan.

## Version identity

Keep three independent values explicit:

| Axis | Example | Advances when |
| --- | --- | --- |
| Grammar crate/release | `0.1.0` | Grammar, binder, printer, compiler or public API changes |
| Output AST schema | `190` | The owned model's wire/schema contract changes |
| Oracle source snapshot | provider ID/timestamp plus SHA-256 | Source card data changes |

A new grammar interpretation or printing fix does not require a schema bump.
A source snapshot can be reprocessed with several grammar releases. Avoid putting
an Oracle date or schema in Cargo build metadata: Cargo ignores `+metadata`
when resolving versions. Compatibility and data provenance belong in the release
manifest. Keep ordinary Cargo SemVer: compatible bug fixes advance patch,
compatible coverage/features advance minor, and breaking public API changes
advance the appropriate incompatible version. For `0.x`, Cargo treats changes
to the nonzero minor as incompatible. Schema compatibility is checked explicitly,
not inferred from the grammar crate's major version.

Use `grammar-v0.1.0` for a code release tag. Identify a native face bundle by
`oracle-<source-id>-s190-g0.1.0-r1`; its GitHub tag can be prefixed `faces-`.
The final revision is only for artifact/export corrections with the same
source/schema/grammar tuple. It must record a changed exporter revision and
checksums; it must never disguise changed grammar semantics under the same
crate version. Immutable release assets are never replaced.

A data-only refresh should not require republishing the grammar crate.
A face bundle always pins the exact grammar crate version and producer commit.
Same-day upstream snapshots must remain distinct through their provider ID or
content hash. Do not use the local export time as the source Oracle version.

## Code release contents

Publish the `oracle-ir-grammar` package to crates.io with the syntax EBNF,
lexical grammar, print model/weights, Rust frontend and its model dependencies.
Include all assets needed to compile and test the packaged crate. Keep full
corpora, native face datasets and MTGish validation snapshots out of the crate.

A GitHub code release contains:

- The exact packaged `.crate`, also published to crates.io.
- A grammar-source archive containing the modular EBNF and printing assets.
- The flattened EBNF export for inspection and tooling. It is not a standalone
  substitute for the Rust semantic implementation.
- `manifest.json` and `SHA256SUMS` naming the commit, package versions, supported
  AST schema, grammar-input hashes, toolchain and build/export options.
- Optional platform-specific CLI archives. They must carry their default
  grammar resources or embed those resources; they cannot require a developer's
  source checkout. Explicit module overrides can remain a development feature.

## Native face bundle

Export from original Oracle text and original source metadata through the native
card/face frontend. Do not copy expected/reference IR or silently fall back to
MTGish when native parsing fails. Reference agreement is a separate report.

Use stable record and face identity, including layout/face position; a face name
alone is not unique. Preserve source identifiers when available. The current
processed local source does not expose a trustworthy upstream snapshot timestamp;
its SHA-256 is authoritative until original provider provenance is captured.

Suggested GitHub assets:

- `faces.jsonl.gz`: unique, fully parsed, validated native faces, with source
  identity and required card/layout context.
- `outcomes.jsonl.gz`: one outcome for every input occurrence. Preserve ambiguity,
  refusal, budget-limit and unsupported statuses, including all supported
  readings rather than selecting the first one. No truncated result is success.
- `coverage.json`: counts and denominators, plus distinct recognition, validation,
  print/reparse and exact-wording results if those gates were performed.
- `manifest.json`: exact inputs, scope policy, grammar release/commit, exporter
  revision, schema and package versions, counts, options and asset hashes.
- `SHA256SUMS`: checksums for every downloadable file.

The data container format has its own `format_version`, separate from the AST
schema. Canonical ordering and deterministic compression make repeated exports
reproducible. Consumers can download a pinned bundle without building the parser.

Example manifest identity (placeholders are intentionally not release metadata):

```json
{
  "format_version": 1,
  "kind": "oracle-grammar-native-faces",
  "grammar": {"crate": "oracle-ir-grammar", "version": "0.1.0", "commit": "<full commit>", "inputs_sha256": "<sha256>"},
  "model": {"schema_version": 190, "packages": {"oracle-ir": "0.1.0", "oracle-ir-contract": "0.1.0"}},
  "source_oracle": {"provider": "<provider>", "snapshot_id": "<provider snapshot or hash>", "sha256": "<sha256>"},
  "population": {"policy": "<explicit input/exclusion policy>", "sha256": "<sha256>"},
  "exporter": {"commit": "<full commit>", "revision": 1},
  "assets": {},
  "counts": {}
}
```

## Publication sequence

1. Prepare and review the release commit, versions, changelog and manifests.
2. Run repository correctness/family gates and package the actual crate files.
   Verify packages from their extracted tarballs, with no sibling checkout or
   path/dependency patch. Smoke-test loading the packaged grammar from a separate
   consumer. If shipping installed binaries, also test without the source tree.
3. For the initial registry bootstrap, publish missing dependencies in order:
   `oracle-ir`, `oracle-ir-contract`, then `oracle-ir-grammar`. Verify registry
   availability between packages. Existing exact dependency releases are reused.
4. Build native exports against the immutable Oracle input and scope manifest;
   validate every accepted face and assert every input has an explicit outcome.
5. Create a draft GitHub release at the exact commit/tag, upload all assets,
   verify checksums, then publish the release. Keep code and data tags distinct
   so code-only and data-only releases do not force each other.
6. After initial crates.io publication, configure Trusted Publishing for the
   repository and release workflow. GitHub Actions obtains a short-lived OIDC
   publishing token; no persistent Cargo token is needed for later releases.

Crates.io currently requires an API token for the first publication of a name;
Trusted Publishing is configured on an existing crate. Release workflows should
be directly triggered by the chosen release tag or explicit dispatch. The
workflow must verify tag/version/commit agreement before any upload. Verification
runs before publication; a retry inspects existing versions/assets rather than
replacing a published version. GitHub release immutability can lock final assets.

## Current readiness audit

- `oracle-ir-grammar` is still `publish = false`, version `0.1.0`.
- Crates.io returned 404 for all three package names during this audit; they
  appear available, but this check does not reserve them.
- No local Cargo publication credentials or token environment variable were
  present. GitHub CLI authentication is available; there are no existing releases.
- `cargo package --list` includes the grammar files today. Package metadata,
  intended include paths and install/runtime behavior still need a release pass.
- The shared contract crate currently embeds historical schemas through sibling
  package paths. Those reads must go through an API provided by `oracle-ir` so
  packaged crates compile independently.
- Current `inspect` is a bounded diagnostic, and `score` compares against a
  reference. Neither is yet the native face release exporter described here.

## Primary documentation

- [Cargo publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [Cargo package verification](https://doc.rust-lang.org/cargo/commands/cargo-package.html)
- [Cargo version metadata](https://doc.rust-lang.org/cargo/reference/manifest.html#the-version-field)
- [Crates.io Trusted Publishing](https://crates.io/docs/trusted-publishing)
- [GitHub releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)
- [GitHub immutable releases](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/prevent-release-changes)
