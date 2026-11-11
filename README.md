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
cargo test --locked
cargo run --locked --features cli --bin probe -- Example 'Draw a card.'
cargo run --locked --features cli --bin grammar-export > /tmp/oracle.ebnf
```

For repeated grammar edits, build the helpers once:

```sh
python3 tools/dev.py build
python3 tools/dev.py probe Example 'Draw a card.'
python3 tools/dev.py family families/revealed-followups.json --compositional
python3 tools/dev.py families
```

Syntax EBNF edits are loaded from disk on each invocation. Rust, lexical EBNF,
print model, toolchain or dependency changes require a rebuild; the helper checks
those inputs before running. A validation-reference update does not require a
rebuild. CLI targets are opt-in, so ordinary tests avoid linking every diagnostic.
Generated files and build receipts stay under ignored `.cache/` and `target/`.
`CARGO_TARGET_DIR` can select a shared local build cache.

## Layout

- `grammar/`: Oracle terminals, phrase composition and inverse semantics.
- `src/`: grammar loader, parser, binder, renderer and model boundary.
- `tests/` and `families/`: checked-in unit/integration assets and phrase families.
- `pins/model.json`: provenance of the current schema implementation.
- `adapters/mtgish/`: independently pinned optional validation reference.
- `tools/`: reusable diagnostics and pin/artifact checks.

[`SPEC.md`](SPEC.md) defines the semantic and performance contracts.
[`AGENTS.md`](AGENTS.md) records contribution hygiene.

## Model and validation reference

This project is intended to own its IR. Today, [`src/model.rs`](src/model.rs)
provides a single compatibility boundary to a pinned MTGish schema and its
validation/canonicalization helpers. The grammar does not use MTGish's source
lowering or text rewrites.

MTGish reference data has a separate lock. Moving its reference revision does
not move the model or change the compiler inputs. See the
[adapter contract](adapters/mtgish/README.md) for the transition to local IR and
an explicit upstream-to-local adapter. Full corpora and held-out data are not
bundled in this repository.

## Verification

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tools -p 'test_*.py'
python3 tools/pin.py check
```

Run `python3 tools/dev.py families` after relevant grammar changes. Performance
comparisons use optimized release binaries and immutable inputs; the helper's
quick build is for correctness and iteration.

Licensed under MIT OR Apache-2.0.
