# Oracle Grammar

A phrase-level grammar for parsing Magic Oracle text into structured rules IR
and printing that IR back to valid Oracle text.

The grammar is the focus of this repository. W3C-style EBNF productions in
[`grammar/`](grammar/) declare their meaning once, in `@sem` comments. The parser
and printer use those same declarations. Context binding resolves references;
spelling metadata records choices without becoming the meaning of a card.

The priorities are, in order:

1. Produce the correct rules-relevant IR.
2. Print valid, correct text that reparses to that IR, including edited IR.
3. Match the card's Oracle wording.
4. Reproduce labels and reminder text.

This is a prototype, with explicit refusals and multiple readings. It is not a
rules execution engine. Recognition, reference agreement, semantic roundtrips
and exact wording are separate measurements.

## Build and use

Rust 1.98.0 and Python 3.11 or newer are required. Dependencies use public HTTPS;
there are no submodules or required corpus downloads.

```sh
python3 tools/dev.py test
cargo run --locked --release --features cli --bin probe -- Example 'Draw a card.'
cargo run --locked --release --features cli --bin grammar-export > /tmp/oracle.ebnf
```

For repeated grammar edits, build the helpers once:

```sh
python3 tools/dev.py build
python3 tools/dev.py probe Example 'Draw a card.'
python3 tools/dev.py family families/revealed-followups.json --compositional
python3 tools/dev.py families
```

`python3 tools/dev.py test` (run from the repository root) runs the full unit suite in the same release profile and CLI
feature configuration as the helpers. Filters stay cheap, for example
`python3 tools/dev.py test tests::revealed_followups` or
`python3 tools/dev.py test bind::optional_entered_card`. Plain `cargo test --locked` also
works when a development-profile run is needed.

Syntax EBNF edits are loaded from disk on each invocation. Rust, lexical EBNF,
print model, toolchain or dependency changes require a rebuild; the helper checks
those inputs before running. A validation-reference update does not require a
rebuild. CLI targets are opt-in, so ordinary tests avoid linking every diagnostic. The
regression suites share one immutable test engine in one executable; tests use
opt-level 1 for the frontend in dev/test and reuse the ordinary dependency builds.
A warm `build` reuses verified helpers without invoking Cargo; each run verifies
the selected executable. Read-only helper runs share a lock and may run in
parallel; builds use an exclusive lock.
Generated files and build receipts stay under ignored `.cache/` and `target/`.
`CARGO_TARGET_DIR` can select a shared local build cache. The helper remembers
its last build target directory for later rebuilds; an explicit environment or
repository Cargo setting takes precedence. The test preset uses that same target
selection, so omitting a one-command environment setting does not create a
second dependency cache. A remembered directory that no longer exists is refused
until `CARGO_TARGET_DIR` selects one. Quick helper builds and the test preset
compile the frontend incrementally; `build --optimized` does not.
`ORACLE_GRAMMAR_THREADS` raises the default of two worker threads for families,
inspection and scoring.

`tools/corpus.py` scores cohorts of an explicit unsealed population against a
baseline. Its inputs and their SHA-256 checksums are named in the ignored local
manifest `.cache/corpus/inputs.json`; `slice`, `score`, `audit`, `queue` and
`accept` compare by occurrence and refuse lost matches, lost parses or limits.
`rules` names the seeded (`m_` and `t_`) rules that a slice's matching faces
need. `tools/mined.py` removes unneeded rules, or adopts a seeded rule into a
syntax module under a descriptive name so that the next audit retires the
original. The audit only sees parses of matching faces: a removal can still drop
a print template, so also run the library tests and compare a render run.

## Layout

- `grammar/`: Oracle terminals, phrase composition and inverse semantics.
- `src/`: grammar loader, parser, binder, renderer and model boundary.
- `src/tests/`, `tests/fixtures/` and `families/`: regression tests and phrase assets.
- `crates/oracle-ir/`: owned typed AST, vocabulary, traversal and frozen wire schemas.
- `crates/oracle-ir-contract/`: shared validation, binding, canonicalization and authoring.
- `pins/model.json`: owned package identities and extraction provenance.
- `adapters/mtgish/`: independently pinned optional validation reference.
- `tools/`: reusable diagnostics and pin/artifact checks.

[`SPEC.md`](SPEC.md) defines the semantic and performance contracts.
[`AGENTS.md`](AGENTS.md) records contribution hygiene.

## Model and validation reference

This project owns its IR in two workspace crates: `oracle-ir` defines the types
and wire contract; `oracle-ir-contract` defines validation, canonicalization and
binding semantics. [`src/model.rs`](src/model.rs) is the frontend import boundary.
Normal builds have no MTGish package dependencies. The initial extraction
preserves schema 190; see [the ownership migration](docs/owned-ir-crates.md).

MTGish reference data has a separate lock. Moving its reference revision does
not move the model or change the compiler inputs. See the
[adapter contract](adapters/mtgish/README.md) for independently versioned reference artifacts and schema compatibility. Full corpora and held-out data are not
bundled in this repository.

## Verification

```sh
cargo fmt --check
python3 tools/dev.py test
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tools -p 'test_*.py'
python3 tools/pin.py check
```

Run `python3 tools/dev.py families` after relevant grammar changes. Performance
comparisons use optimized release binaries and immutable inputs; the helper's
quick build is for correctness and iteration.

Licensed under MIT OR Apache-2.0.
