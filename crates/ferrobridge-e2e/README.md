# ferrobridge-e2e

End-to-end tests: the bridges produced by `ferrobridge generate` are compiled together with real C++ code and called from Rust tests. There are no hand-written bridges and no `unsafe`.

| Fixture | C++ | Generated files | Tests |
|---|---|---|---|
| classes | `tests/fixtures/classes/{input.h,impl.cc}` | `src/generated/classes/` | `tests/classes.rs` |
| enums | `tests/fixtures/enums/{input.h,impl.cc}` | `src/generated/enums/` | `tests/enums.rs` |
| strings | `tests/fixtures/strings/{input.h,impl.cc}` | `src/generated/strings/` | `tests/strings.rs` |
| ownership | `tests/fixtures/ownership/{input.h,impl.cc,ferrobridge.toml}` | `src/generated/ownership/` | `tests/ownership.rs` |
| vectors | `tests/fixtures/vectors/{input.h,impl.cc}` | `src/generated/vectors/` | `tests/vectors.rs` |
| slices | `tests/fixtures/slices/{input.h,impl.cc,ferrobridge.toml}` | `src/generated/slices/` | `tests/slices.rs` |
| inline_and_constants | `tests/fixtures/inline_and_constants/{input.h,impl.cc}` | `src/generated/inline_and_constants/` | `tests/inline_and_constants.rs` |
| value_types | `tests/fixtures/value_types/{input.h,impl.cc,ferrobridge.toml}` | `src/generated/value_types/` | `tests/value_types.rs` |
| operators | `tests/fixtures/operators/{input.h,impl.cc}` | `src/generated/operators/` | `tests/operators.rs` |
| no_exceptions | `tests/fixtures/no_exceptions/{input.h,impl.cc,ferrobridge.toml}` | `src/generated/no_exceptions/` | `tests/no_exceptions.rs` |
| multi_header | `tests/fixtures/multi_header/{geometry.h,input.h,impl.cc}` | `src/generated/multi_header/` | `tests/multi_header.rs` |

## Regenerating the generated files

`src/generated/` is never edited by hand. When the generator or a fixture header changes, run this from the repository root:

```bash
for f in classes enums strings ownership vectors slices inline_and_constants multi_header value_types operators no_exceptions; do
  headers=(tests/fixtures/$f/*.h); include=()
  for h in "${headers[@]}"; do include+=(--include-as "$f/$(basename "$h")"); done
  config=(); [ -f tests/fixtures/$f/ferrobridge.toml ] && config=(--config tests/fixtures/$f/ferrobridge.toml)
  cargo run -q -p ferrobridge-cli -- generate "${headers[@]}" \
    --out-dir crates/ferrobridge-e2e/src/generated/$f \
    "${include[@]}" --shim-include $f/shim.h --shim-namespace ferrobridge::$f "${config[@]}"
done
```

All `.h` files in a fixture directory (not subdirectories) are the bridge's headers; most fixtures have only `input.h`. If a fixture has a `ferrobridge.toml` next to it, it is passed with `--config` (currently `ownership`, `slices`, `value_types` and `no_exceptions`). When adding a fixture, update both the `FIXTURES` list in `build.rs` and the `E2E_FIXTURES` list in `ferrobridge-cli/tests/generate.rs`.

The `generated_files_match_the_committed_e2e_files` test in `ferrobridge-cli` fails when the committed files differ from the generator's current output.
