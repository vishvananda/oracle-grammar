# Compiled guarded-trigger Wasm prototype

The host compiler loads and checks the ordinary lexical grammar and elaborates
each selected `@sem` against the native schema. The profile supplies examples
of the supported grammar shapes. The compiler keeps a spelling derivation of
each reading in those examples, checks the dependency closure, and stores the
resulting productions and typed templates. The examples are build inputs. The
module embeds the production table and uses the same parser, inverse and native
IR validation as the host. Runtime initialization loads the grammar table and
the frozen schema descriptors, then builds token and print indexes.

The slice supports guarded self-entry triggers with the selected condition
forms, token recipes and numeric operands. It can read a different card name
or an edited amount in those forms. Coverage follows the included production
table. The packed full-grammar baseline has a wider language.
[Prototype profile](profile.json)

From the repository root, with the `wasm32-unknown-unknown` Rust target installed:

```sh
mkdir -p .cache/guarded-wasm
cargo run --locked --release --features cli,compiled-grammar \
  --bin compile-grammar-slice -- examples/guarded-wasm/profile.json \
  .cache/guarded-wasm/grammar.json
ORACLE_SLICE_PATH="$PWD/.cache/guarded-wasm/grammar.json" \
  CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/.cache/guarded-wasm/target}" \
  cargo build --locked --release --target wasm32-unknown-unknown \
  --manifest-path examples/guarded-wasm/Cargo.toml
node examples/guarded-wasm/measure.mjs \
  "${CARGO_TARGET_DIR:-$PWD/.cache/guarded-wasm/target}/wasm32-unknown-unknown/release/oracle_guarded_wasm.wasm"
```

The module exports `init`, `input`, `run`, `output`, and `memory`. `init`
returns the byte count of its JSON response. Pass a UTF-8 request length to
`input`, write the request at its returned address, then call `run` for the
response length and `output` for its address. An input address of zero means
the request exceeded 1 MiB. Read each response before issuing another call.
Requests include `name` and optionally `types`. The `parse` operation accepts
`text` and returns `readings`. `print` accepts one `reading` and returns verified
`text` when it can print it. `verify` accepts both and checks membership in the
readings obtained by reparsing. Errors return an `error` field.

The measurement script checks edited operands, verified output and whole-input
refusals before timing. It reports raw and Brotli quality 9 sizes, cold module
loading and grammar initialization, linear memory before and after warm work,
and per-call medians for parse, verified print and standalone verification. A
second module path enables an interleaved comparison with the packed baseline.
