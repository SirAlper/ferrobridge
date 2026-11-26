# Plan: Milestone 1 — Header'ı okuma

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 1 — Header'ı okuma ("Kullanıcı bir C++ header'ını verir, araç içinde ne gördüğünü (fonksiyonlar, tipler) okunabilir biçimde gösterir")
**Complexity**: Medium
**Durum**: Tamamlandı (2026-11-26). Sapmalar planın sonunda.

## Summary
`ferrobridge parse <header>` komutu, libclang ile bir C++ header'ını okuyup içindeki **serbest fonksiyonları** (namespace'leriyle) ve bunların **parametre/dönüş tiplerini** IR'a çevirir ve IR'ı okunabilir (girintili) JSON olarak basar. Bu milestone'da sınıf, template ve enum'lar okunmaz ama **sessizce atlanmaz**: IR'a "desteklenmiyor" öğesi olarak, konumu ve nedeniyle birlikte yazılır. Kod üretimi yok.

## Kapsam
**Var:** serbest fonksiyonlar · namespace'ler (iç içe dahil) · primitif tipler (`void`, `bool`, `char`, işaretli/işaretsiz tamsayılar, `float`, `double`) · `const`, pointer (`T*`), lvalue/rvalue referans (`T&`, `T&&`) · `noexcept` bilgisi · aynı isimli overload'lar (ayrı öğeler olarak) · desteklenmeyen yapılar için açıklamalı öğe · libclang hata tanıları
**Yok (sonraki milestone'lar):** sınıf/struct üyeleri, enum, template, `std::string`/`std::vector` eşlemesi, sahiplik analizi, kod üretimi, `compile_commands.json` okuma

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Naming | `crates/ferrobridge-cli/src/main.rs:15` | Alt komutlar clap derive `enum Command` varyantı; yardım metni Türkçe doc yorumu |
| Errors | `crates/ferrobridge-parser/src/lib.rs:14` | Crate başına tek `thiserror` enum'u; Türkçe `#[error(...)]` mesajı; `map_err` ile çeviri (`lib.rs:28`) |
| Unsafe | `crates/ferrobridge-parser/src/lib.rs:40` | Her `unsafe` bloğun üstünde `// SAFETY:` gerekçesi; libclang kaynakları aynı fonksiyonda/`Drop` içinde bir kez serbest bırakılır |
| libclang yükleme | `crates/ferrobridge-parser/src/lib.rs:24` | Her public giriş noktası önce `ensure_loaded()` çağırır |
| CLI hata çıktısı | `crates/ferrobridge-cli/src/main.rs:35` | `eprintln!("✗ {error}")` + ipucu satırı, `ExitCode::FAILURE` |
| IR | `crates/ferrobridge-ir/src/lib.rs:16` | `serde` derive'lı, doc yorumlu public tipler; `IR_FORMAT_VERSION` (`lib.rs:12`) |
| Tests | `crates/ferrobridge-ir/src/lib.rs:36` | Modül içi `#[cfg(test)] mod tests`; testlerde `unwrap()` serbest. Fixture/snapshot testleri için henüz örnek yok → bu plan ilkini kuruyor (`insta`) |
| Logging | — | Projede logging yok; bu milestone'da da eklenmiyor |

## Files to Change
| File | Action | Why |
|---|---|---|
| `Cargo.toml` | UPDATE | `ferrobridge-ir` workspace bağımlılığı, `insta` ve `serde_json` sürümleri |
| `crates/ferrobridge-ir/src/lib.rs` | UPDATE | Modül kökü; alt modülleri dışa aç |
| `crates/ferrobridge-ir/src/item.rs` | CREATE | `Module.items`, `Item` (`Function`, `Unsupported`), `Function`, `Param`, `SourceLocation` |
| `crates/ferrobridge-ir/src/types.rs` | CREATE | `Type` (primitifler, `Pointer`, `LValueRef`, `RValueRef`, `Unsupported`) ve `const` bilgisi |
| `crates/ferrobridge-parser/Cargo.toml` | UPDATE | `ferrobridge-ir` bağımlılığı; `insta`, `serde_json` dev-bağımlılığı |
| `crates/ferrobridge-parser/src/lib.rs` | UPDATE | `parse_header(path, &ParseOptions) -> Result<Module, ParserError>` giriş noktası; yeni hata varyantları |
| `crates/ferrobridge-parser/src/clang.rs` | CREATE | İnce güvenli sarmalayıcı: `Index`, `TranslationUnit` (RAII), `Cursor`, `Type`, çocukları gezme, tanılar (bindgen `clang.rs` örnek alınır) |
| `crates/ferrobridge-parser/src/lower.rs` | CREATE | Cursor → IR dönüşümü; yalnızca ana dosyadaki bildirimler; desteklenmeyenleri `Item::Unsupported` yapma |
| `crates/ferrobridge-parser/tests/fixtures.rs` | CREATE | Her fixture için `parse_header` → JSON snapshot (`insta`) |
| `tests/fixtures/free_functions/input.h` | CREATE | Basit fonksiyonlar, isimsiz parametre, `noexcept`, overload |
| `tests/fixtures/primitive_types/input.h` | CREATE | Tüm primitifler, `<cstdint>` sabit genişlikli tipler, `const`, pointer, referanslar |
| `tests/fixtures/namespaces/input.h` | CREATE | İç içe namespace, aynı isimli fonksiyon farklı namespace'te |
| `tests/fixtures/unsupported/input.h` | CREATE | Sınıf, template fonksiyon, enum → `Unsupported` öğeleri |
| `tests/fixtures/syntax_error/input.h` | CREATE | Bozuk header → tanı içeren hata |
| `crates/ferrobridge-cli/Cargo.toml` | UPDATE | `serde_json` bağımlılığı |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | `parse <header> [--std <c++17>] [-- <clang argümanları>]` alt komutu |
| `crates/ferrobridge-cli/tests/parse.rs` | CREATE | Derlenmiş `ferrobridge` ikilisini (`CARGO_BIN_EXE_ferrobridge`) çalıştıran uçtan uca testler |
| `README.md` | UPDATE | `parse` kullanım örneği |

## Tasks
Her görev TDD ile: önce başarısız test, sonra kod, sonra doğrulama.

### Task 1: IR tipleri
- **Action**: `Module { format_version, items }`, `Item` (serde `tag = "kind"`), `Function { name, namespace: Vec<String>, params, return_type, noexcept, location }`, `Param { name: Option<String>, ty }`, `Type` (C++ yazımı + `size_bytes` saklanır; `int`/`long` gibi platforma bağlı tipler ayrıca işaretlenir), `Unsupported { kind, name, reason, location }`, `SourceLocation { file, line, column }` (dosya yolu header'a göre göreli). `IR_FORMAT_VERSION` → 1.
- **Mirror**: `crates/ferrobridge-ir/src/lib.rs:16` (serde derive + doc yorumu)
- **Validate**: `cargo test -p ferrobridge-ir` (her varyant için JSON gidiş-dönüş testi)

### Task 2: Güvenli libclang sarmalayıcısı
- **Action**: `clang.rs` içinde `Index` ve `TranslationUnit` için `Drop` ile serbest bırakma; `TranslationUnit::parse(path, args)`; `Cursor::{kind, spelling, location, is_in_main_file, children, result_type, arguments, exception_spec}`; `Type::{kind, spelling, size_of, pointee, is_const}`; `clang_visitChildren` için closure köprüsü; hata/fatal tanılarını `Vec<Diagnostic>` olarak toplama. macOS'ta `-isysroot` verilmemişse `xcrun --show-sdk-path` ile otomatik eklenir (`<cstdint>` bulunabilsin diye).
- **Mirror**: `crates/ferrobridge-parser/src/lib.rs:40` (`// SAFETY:` + tek seferlik serbest bırakma), `lib.rs:24` (`ensure_loaded`)
- **Validate**: `cargo test -p ferrobridge-parser clang::` (geçerli header açılıyor; olmayan dosya → hata; bozuk header → tanı listesi)

### Task 3: Cursor → IR dönüşümü
- **Action**: `lower.rs` ana dosyadaki üst seviye ve namespace içindeki `FunctionDecl`'ları `Item::Function`'a çevirir. Sınıf, struct, enum, template ve tanınmayan tipler `Item::Unsupported` (neden metniyle) olur; hiçbir şey sessizce atlanmaz. `#include` edilen başka dosyalardaki bildirimler alınmaz.
- **Mirror**: Task 2 sarmalayıcısı; hatalar `ParserError` üzerinden (`lib.rs:14`)
- **Validate**: `cargo test -p ferrobridge-parser --test fixtures` + `cargo insta review` (5 fixture snapshot'ı elle incelenip kabul edilir)

### Task 4: `parse_header` giriş noktası ve hatalar
- **Action**: `ParseOptions { std: String (varsayılan "c++17"), extra_args: Vec<String> }`. Yeni hatalar: `FileNotFound(PathBuf)`, `ParseFailed { diagnostics }` (her tanı `dosya:satır:sütun: mesaj` biçiminde). Uyarı seviyesindeki tanılar hata sayılmaz.
- **Mirror**: `crates/ferrobridge-parser/src/lib.rs:14`
- **Validate**: `syntax_error` fixture'ı `ParseFailed` döndürür ve mesaj satır numarası içerir

### Task 5: `ferrobridge parse` komutu
- **Action**: `Command::Parse { header, std, clang_args }`. Başarıda girintili JSON'u stdout'a basar, çıkış kodu 0. Hatada her tanıyı `✗` ile stderr'e basar, çıkış kodu 1. libclang yoksa `doctor` ile aynı ipucunu gösterir.
- **Mirror**: `crates/ferrobridge-cli/src/main.rs:15` ve `main.rs:35`
- **Validate**: `cargo test -p ferrobridge-cli` (başarılı parse → geçerli JSON ve beklenen fonksiyon adı; bozuk header → çıkış kodu 1); `cargo run -p ferrobridge-cli -- parse tests/fixtures/free_functions/input.h`

### Task 6: Belgeler
- **Action**: README'ye `parse` örneği; PRD'de Milestone 1 durumu ve Obsidian notları (04 İlerleme günlüğü, 02 faz tablosu) güncellenir.
- **Validate**: README'deki komut kopyalanıp çalıştırıldığında çıktı veriyor

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo insta test --review            # snapshot değişikliklerini incele
cargo run -p ferrobridge-cli -- parse tests/fixtures/free_functions/input.h
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| Homebrew libclang macOS SDK header'larını (`<cstdint>`) bulamaz | Yüksek | `-isysroot $(xcrun --show-sdk-path)` otomatik eklenir; kullanıcı `--` ile kendi bayraklarını verebilir; fixture ile test edilir |
| Snapshot'lar libclang sürümüne veya platforma göre değişir (Apple 21 vs Homebrew 23, `long` boyutu) | Orta | Fixture'lar sabit genişlikli tipler ve `int` kullanır; snapshot'ta mutlak yol yok; testler `doctor` ile aynı libclang'ı kullanır |
| `clang-sys` `runtime` modunda kütüphane iş parçacığına özel yüklenir; paralel testlerde yüklenmemiş iş parçacığı çöker | Orta | Her public giriş noktası `ensure_loaded()` çağırır; sarmalayıcı tipleri `Send` yapılmaz (iş parçacığı dışına çıkamaz) |
| libclang C API'si bir bilgiyi vermez (ör. `noexcept` ayrıntısı) | Düşük | ADR-0001'deki kaçış yolu; M1'de gereken bilgiler (isim, tip, boyut, exception spec, konum) C API'de mevcut, Task 2'de doğrulanır |
| `clang_visitChildren` closure köprüsünde UB | Orta | bindgen'in kanıtlanmış deseni; panik FFI sınırını geçmez (`catch_unwind`); `/ecc:security-review` ile incelenir |

## Acceptance
- [ ] `ferrobridge parse <header>` 5 fixture için beklenen çıktıyı verir (snapshot'lar elle incelendi)
- [ ] Desteklenmeyen her yapı çıktıda `Unsupported` öğesi olarak, nedeni ve konumuyla görünür
- [ ] Bozuk header'da satır numaralı hata mesajı ve çıkış kodu 1
- [ ] Validation komutlarının hepsi geçer
- [ ] Yeni `unsafe` blokların hepsi `// SAFETY:` gerekçeli ve `/ecc:security-review`'dan geçti
- [ ] Patterns mirrored, not reinvented

## Uygulama notları (2026-11-26)
- **Sabit genişlikli tip tespiti plandan farklı yapıldı.** libclang, `std::int32_t`'yi (std içindeki `using ::int32_t;`) bildirimi olmayan "unexposed" bir tip olarak veriyor; typedef zinciri orada izlenemiyor. Tespit ad üzerinden yapılıyor (`int32_t`, `std::uint8_t`...) ve gerçek boyutla karşılaştırılıyor.
- **İntegration test dosyalarında** `#![allow(clippy::unwrap_used)]` gerekti: `clippy.toml` test istisnası yalnızca `#[test]` fonksiyonlarını kapsıyor, yardımcı fonksiyonları değil.
- **Güvenlik incelemesi:** 35 `unsafe` bloğun hepsi `clang.rs`'de ve gerekçeli. Bulunan tek sorun düzeltildi: libclang hata kodu döndürüp yine de translation unit verirse sızıntı olurdu. `cargo audit` / `cargo deny` kurulu olmadığı için bağımlılık taraması yapılmadı.
- **Platform kontrolü:** Snapshot'lar hem Homebrew libclang 23.1.6 hem Apple libclang 21.0.0 ile aynı çıktı veriyor. `char` işaretliliği platforma bağlı (macOS arm64'te işaretli); Linux CI'da farklı çıkabilir.
