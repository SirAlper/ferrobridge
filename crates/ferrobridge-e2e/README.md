# ferrobridge-e2e

Uçtan uca testler: `ferrobridge generate`'in ürettiği bridge'ler gerçek C++ koduyla derlenir ve Rust testlerinden çağrılır. Elle yazılmış bridge veya `unsafe` yoktur.

| Fixture | C++ | Üretilmiş dosyalar | Testler |
|---|---|---|---|
| classes | `tests/fixtures/classes/{input.h,impl.cc}` | `src/generated/classes/` | `tests/classes.rs` |
| enums | `tests/fixtures/enums/{input.h,impl.cc}` | `src/generated/enums/` | `tests/enums.rs` |
| strings | `tests/fixtures/strings/{input.h,impl.cc}` | `src/generated/strings/` | `tests/strings.rs` |
| ownership | `tests/fixtures/ownership/{input.h,impl.cc,ferrobridge.toml}` | `src/generated/ownership/` | `tests/ownership.rs` |
| vectors | `tests/fixtures/vectors/{input.h,impl.cc}` | `src/generated/vectors/` | `tests/vectors.rs` |

## Üretilmiş dosyaları yenilemek

`src/generated/` elle düzenlenmez. Üretici değişince ya da fixture header'ları değişince depo kökünden:

```bash
for f in classes enums strings ownership vectors; do
  config=(); [ -f tests/fixtures/$f/ferrobridge.toml ] && config=(--config tests/fixtures/$f/ferrobridge.toml)
  cargo run -q -p ferrobridge-cli -- generate tests/fixtures/$f/input.h \
    --out-dir crates/ferrobridge-e2e/src/generated/$f \
    --include-as $f/input.h --shim-include $f/shim.h --shim-namespace ferrobridge::$f "${config[@]}"
done
```

Fixture'ın yanında `ferrobridge.toml` varsa `--config` ile verilir (şu an yalnızca `ownership`). Yeni fixture eklerken `build.rs`'teki `FIXTURES` listesini ve `ferrobridge-cli/tests/generate.rs`'teki `E2E_FIXTURES` listesini birlikte güncelleyin.

`ferrobridge-cli`'deki `generated_files_match_the_committed_e2e_files` testi, commit'li dosyalar üreticinin güncel çıktısından farklıysa başarısız olur.
