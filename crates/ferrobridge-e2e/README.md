# ferrobridge-e2e

Uçtan uca testler: `ferrobridge generate`'in ürettiği bridge'ler gerçek C++ koduyla derlenir ve Rust testlerinden çağrılır. Elle yazılmış bridge veya `unsafe` yoktur.

| Fixture | C++ | Üretilmiş dosyalar | Testler |
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

## Üretilmiş dosyaları yenilemek

`src/generated/` elle düzenlenmez. Üretici değişince ya da fixture header'ları değişince depo kökünden:

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

Fixture dizinindeki bütün `.h` dosyaları (alt dizinler hariç) bridge'in header'larıdır; çoğunda yalnızca `input.h` var. Fixture'ın yanında `ferrobridge.toml` varsa `--config` ile verilir (şu an `ownership`, `slices`, `value_types` ve `no_exceptions`). Yeni fixture eklerken `build.rs`'teki `FIXTURES` listesini ve `ferrobridge-cli/tests/generate.rs`'teki `E2E_FIXTURES` listesini birlikte güncelleyin.

`ferrobridge-cli`'deki `generated_files_match_the_committed_e2e_files` testi, commit'li dosyalar üreticinin güncel çıktısından farklıysa başarısız olur.
