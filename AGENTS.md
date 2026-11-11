# Repository guidance

Read [SPEC.md](SPEC.md) for the current architecture and semantic contracts.
This repository contains the phrase-level grammar and its frontend. Keep work
focused on that pipeline and make coherent commits.

## Semantic checks

- Whole input must be consumed; opaque or recovered effects cannot claim success.
- Return every distinct supported reading. Never choose semantics by first match.
- Preserve source, ability holder, actor, owner, controller and result identities.
- Print edited or synthesized IR from its current operands, then verify reparse.
- Keep exact wording, semantic roundtrip and reference equality separate.
- Declare each production's meaning once in `@sem`, shared by both directions.
- Validate lexical EBNF before using a fast path.
- Use original Oracle words; do not apply MTGish's source text rewrites.

## Model and reference

`src/model.rs` is the schema compatibility boundary. `pins/model.json` and
`Cargo.lock` pin its implementation. The MTGish reference lock lives separately
in `adapters/mtgish/reference.json`; reference-only changes must not move the
model or trigger a compiler rebuild. No local dependency patches or replacements.

The intended architecture has local IR ownership and an optional MTGish adapter
for validation. Keep upstream-specific Source translation out of grammar rules.
Use isolated checkouts for upstream experiments; preserve other workers' changes.

Full corpora and held-out data are not bundled here. During rule authoring, do
not inspect held-out cards. Audits require an explicit allowed population and
immutable inputs. Both-unsupported never counts as semantic agreement.

## Hygiene and cycle time

Use safe Rust, typed semantic contracts and explicit byte/depth/work/candidate
bounds. Rust files target 800 lines, hard limit 1,500; functions have at most
200 lines and seven parameters. Keep one implementation per responsibility.

Group fixes before relevant family validation. Use `python3 tools/dev.py build`
once after compiled-input changes, then reuse the helpers for syntax edits.
CLI binaries are feature-gated. Default-grammar tests share the read-only engine
in `src/test_support.rs`; use a fresh engine for custom grammars or mutable settings.
Add regression tests under `src/tests/`; root `tests/` holds data fixtures. Respect `CARGO_TARGET_DIR` for shared caches.
Do not run a whole corpus or rebuild at every grammar-only checkpoint.

Run `cargo fmt --check`, `python3 tools/dev.py test` (the locked release library test preset), and
`cargo clippy --locked --all-targets --all-features -- -D warnings`, plus
`python3 -m unittest discover -s tools -p 'test_*.py'`,
`python3 tools/pin.py check` and `python3 tools/hygiene.py`.
Run affected families after rule changes. Keep generated output under ignored
`.cache/` or `target/`. Never commit build products, full corpora or machine paths.

Measure immutable optimized release binaries with repeated, interleaved runs
and medians. Separate load, parse, print, reparse and corpus I/O. Keep work and
allocation evidence; allocation builds have separate timing baselines.
