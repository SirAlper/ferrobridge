# ferrobridge-e2e

Uçtan uca testler: `ferrobridge generate`'in ürettiği bridge'ler gerçek C++ koduyla derlenir ve Rust testlerinden çağrılır. Elle yazılmış bridge veya `unsafe` yoktur.

| Fixture | C++ | Üretilmiş dosyalar | Testler |
|---|---|---|---|
| classes | `tests/fixtures/classes/{input.h,impl.cc}` | `src/generated/classes/` | `tests/classes.rs` |
| enums | `tests/fixtures/enums/{input.h,impl.cc}` | `src/generated/enums/` | `tests/enums.rs` |
| strings | `tests/fixtures/strings/{input.h,impl.cc}` | `src/generated/strings/` | `tests/strings.rs` |

## Üretilmiş dosyaları yenilemek

`src/generated/` elle düzenlenmez. Üretici değişince ya da fixture header'ları değişince depo kökünden:

```bash
for f in classes enums strings; do
  cargo run -q -p ferrobridge-cli -- generate tests/fixtures/$f/input.h \
    --out-dir crates/ferrobridge-e2e/src/generated/$f \
    --include-as $f/input.h --shim-include $f/shim.h --shim-namespace ferrobridge::$f
done
```

`ferrobridge-cli`'deki `generated_files_match_the_committed_e2e_files` testi, commit'li dosyalar üreticinin güncel çıktısından farklıysa başarısız olur.
