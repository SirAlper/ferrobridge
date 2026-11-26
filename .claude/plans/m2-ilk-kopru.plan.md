# Plan: Milestone 2 — İlk köprü

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 2 — İlk köprü ("Basit bir C++ sınıfı, elle bridge yazılmadan Rust testinden çağrılabilir")
**Complexity**: Large
**Durum**: Tamamlandı (2026-11-27). Sapmalar planın sonunda.

## Summary
`ferrobridge generate <header> --out-dir <dizin>` komutu, bir header'dan **cxx bridge modülü** (`bridge.rs`) ve gereken **C++ shim** dosyalarını (`shim.h`, `shim.cc`) üretir. Sınıflar (opak tip olarak), metotlar, constructor'lar, enum'lar, serbest fonksiyonlar ve `std::string` desteklenir. Üretilen kod, ayrı bir test crate'inde gerçek bir C++ kütüphanesiyle **derlenir ve Rust testlerinden çağrılır**; elle tek satır bridge yazılmaz. Sahiplik açıklamaları (M3) ve idiomatik güvenli sarmalayıcı (M4) bu milestone'da yok; çıktı cxx seviyesinde kalır.

## Kapsam
**Var:**
- Sınıf/struct: opak tip (`type Vehicle;`). Yalnızca **public** üyeler.
- Metotlar: `const` → `self: &T`, const olmayan → `self: Pin<&mut T>`. Virtual metotlar çağrılabilir (override edilemez).
- Constructor'lar: shim fabrika fonksiyonu → `UniquePtr<T>`. Abstract sınıf ve `= delete` constructor'lar için üretilmez.
- Statik metotlar: shim serbest fonksiyonu.
- Enum'lar (scoped ve unscoped): cxx "extern enum" deseni; değerler C++ ile derleme zamanında doğrulanır.
- Serbest fonksiyonlar (M1'den) ve namespace'ler (`#[namespace = "..."]`).
- Tipler: primitifler · `const T&` (T: primitif, sınıf, `std::string`) → `&T` · sınıf için `T&` → `Pin<&mut T>` · `std::string` dönüşü → `UniquePtr<CxxString>` · değerle dönen sınıf → shim'de `unique_ptr`'a sarılır · `std::unique_ptr<T>` dönüşü → `UniquePtr<T>`.
- Overload'lar: Rust adı parametre tiplerinden türetilir (`clamp_i32_i32_i32`, `clamp_f64_f64_f64`), C++ adı `#[cxx_name]` ile korunur.
- Exception: `noexcept` olmayan her fonksiyon `Result<T>` döner (güvenlik kuralı 4). İdiomatik hata tipi M4'te.
- Desteklenmeyen her öğe/üye: üretilen dosyada yorum + `generate` çıktısında açıklamalı uyarı.

**Yok (gerekçeli):**
- Ham pointer'lar ve primitif `T&` (ör. `int& out`) → **M3** (sahiplik açıklaması gerekli; `03 Mimari` tip tablosu kararı)
- Sınıfı değerle parametre almak → kopyalama/taşıma semantiği gerekir, **M3/M4**
- `std::vector` → **M2'den çıkarıldı**, M3'e kaydırılması öneriliyor (eleman tipi kısıtları ayrı iş)
- Kalıtım (base sınıf metotları), operatörler, dönüşüm fonksiyonları, alanlara (field) erişim, template → v0.2
- `build.rs` yardımcı crate'i (`ferrobridge-build`) → **M5**

## Karar gerektiren noktalar (onayınla kabul edilir)
1. **Minimum libclang 16.** `= delete` constructor'ları tespit etmek için gereken fonksiyon libclang 16 ile geldi. `clang-sys` özelliği `clang_10_0` → `clang_16_0` yükseltilir. PRD'deki "minimum LLVM sürümü" açık sorusu böylece **16** olarak kapanır (CI 21–23'ü test edecek).
2. **ADR-0004: Üretilen kod dosyaya yazılır ve commit'lenir.** (03 Mimari'deki taslak ADR-003.) Kod üretimi `build.rs` içinde gizlice değil, `generate` komutuyla yapılır; kullanıcı üretilen bridge'i görür, review eder, sürüm kontrolünde tutar.
3. **ADR-0005: Overload adlandırma kuralı.** Tek tanımlı isim olduğu gibi kalır; aynı isimden birden fazla varsa *hepsi* parametre tiplerinden türetilmiş sonek alır. Header'a yeni overload eklenince var olan adların değişmesi bilinçli bir bedel olarak kaydedilir.

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Naming | `crates/ferrobridge-cli/src/main.rs:18` | Yeni alt komut `Command` enum'una varyant olarak eklenir; Türkçe doc yorumu yardım metni olur |
| Errors | `crates/ferrobridge-parser/src/lib.rs:66` | Crate başına tek `thiserror` enum'u, Türkçe mesaj |
| CLI hata çıktısı | `crates/ferrobridge-cli/src/main.rs:78` | `report()`: `✗` + ayrıntı satırları, `ExitCode::FAILURE` |
| Unsafe | `crates/ferrobridge-parser/src/clang.rs` | Yeni libclang çağrıları yalnızca `clang.rs`'e, `// SAFETY:` gerekçesiyle |
| IR | `crates/ferrobridge-ir/src/item.rs:10` | `#[serde(tag = "kind", rename_all = "snake_case")]` enum, doc yorumlu alanlar |
| Desteklenmeyen | `crates/ferrobridge-parser/src/lower.rs:17` | `REASON_*` sabitleri; hiçbir şey sessizce atlanmaz |
| Tests | `crates/ferrobridge-parser/tests/fixtures.rs:20` | Fixture header → `insta` snapshot; integration test dosyasında `#![allow(clippy::unwrap_used)]` |
| Uçtan uca | `crates/ferrobridge-cli/tests/parse.rs:17` | `CARGO_BIN_EXE_ferrobridge` ile gerçek ikili |
| Logging | — | Logging yok; uyarılar stderr'e `report()` deseniyle |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge-ir/src/item.rs` | UPDATE | `Item::Class`, `Item::Enum`; `Method`, `Constructor`, `EnumVariant` |
| `crates/ferrobridge-ir/src/types.rs` | UPDATE | `Type::Record { name, namespace }`, `Type::Enum`, `Type::String`, `Type::UniquePtr`; `IR_FORMAT_VERSION` → 2 |
| `crates/ferrobridge-parser/Cargo.toml` | UPDATE | `clang-sys` özelliği `clang_16_0` |
| `crates/ferrobridge-parser/src/clang.rs` | UPDATE | Erişim belirteci, const/static/virtual/pure/deleted, abstract, enum değerleri, template argümanları |
| `crates/ferrobridge-parser/src/lower.rs` | UPDATE | Sınıf üyeleri, constructor, enum, `std::string`/`std::unique_ptr` tanıma |
| `crates/ferrobridge-parser/tests/snapshots/*.snap` | UPDATE | IR v2 ile M1 snapshot'ları değişir (özellikle `unsupported`) |
| `tests/fixtures/classes/input.h` + `impl.cc` | CREATE | Sınıf, metot, constructor, statik, virtual, abstract, deleted ctor, değerle dönüş |
| `tests/fixtures/enums/input.h` | CREATE | Scoped/unscoped, açık değerler, farklı alt tip |
| `tests/fixtures/strings/input.h` + `impl.cc` | CREATE | `const std::string&` parametre, `std::string` dönüşü |
| `crates/ferrobridge-analysis/src/lib.rs` | UPDATE | `check(&Module) -> BridgePlan`: neyin bağlanabileceğine karar verir, uyarıları toplar (güvenlik kuralları burada) |
| `crates/ferrobridge-analysis/src/naming.rs` | CREATE | Overload sonekleri, Rust anahtar kelimeleriyle çakışma (`r#type` yerine `type_`) |
| `crates/ferrobridge-codegen/src/lib.rs` | UPDATE | `generate(&BridgePlan, &CodegenOptions) -> GeneratedFiles` |
| `crates/ferrobridge-codegen/src/bridge.rs` | CREATE | `#[cxx::bridge]` modülü (`quote` + `prettyplease`) |
| `crates/ferrobridge-codegen/src/shim.rs` | CREATE | `shim.h` / `shim.cc`: constructor, statik metot, değerle dönüş sarmalayıcıları |
| `crates/ferrobridge-codegen/tests/fixtures.rs` | CREATE | Her fixture için üretilen 3 dosyanın snapshot'ı |
| `crates/ferrobridge-e2e/` | CREATE | `publish = false` test crate'i: `build.rs` (`cxx-build`) üretilmiş dosyaları ve fixture `impl.cc`'leri derler; `tests/` Rust'tan çağırır |
| `crates/ferrobridge-e2e/src/generated/` | CREATE | `ferrobridge generate` çıktısı (commit'li) |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | `generate` alt komutu |
| `crates/ferrobridge-cli/tests/generate.rs` | CREATE | Uçtan uca: dosyalar yazılıyor, uyarılar basılıyor, e2e'deki commit'li dosyalarla aynı |
| `Cargo.toml` | UPDATE | `ferrobridge-analysis`, `ferrobridge-codegen` workspace bağımlılıkları; `cxx`, `cxx-build`, `quote`, `proc-macro2`, `prettyplease`, `syn` sürümleri |
| `docs/adr/0004-*.md`, `docs/adr/0005-*.md`, `docs/adr/README.md` | CREATE / UPDATE | Kararlar 2 ve 3 |
| `README.md`, PRD, `CLAUDE.md` | UPDATE | `generate` kullanımı, M2 durumu, minimum libclang 16 |

## Tasks
Her görev TDD ile: önce başarısız test, sonra kod, sonra doğrulama.

### Task 1: IR v2
- **Action**: `Class { name, namespace, is_abstract, constructors, methods, location }`, `Method { name, params, return_type, is_const, is_static, is_virtual, noexcept, location }`, `Constructor { params, noexcept, location }`, `Enum { name, namespace, scoped, underlying: Type, variants: [{ name, value: i64 }] }`. Yeni tipler: `Record`, `Enum`, `String`, `UniquePtr`. Desteklenmeyen sınıf üyeleri sınıf içinde `unsupported_members` olarak tutulur.
- **Mirror**: `crates/ferrobridge-ir/src/item.rs:10`
- **Validate**: `cargo test -p ferrobridge-ir` (yeni her varyant için JSON gidiş-dönüş)

### Task 2: Parser: sınıflar, enum'lar, std tipleri
- **Action**: `clang.rs`'e gereken çağrılar; `lower.rs` sınıf üyelerini (yalnızca public) gezer, constructor/metot/statik/virtual/abstract/deleted bilgisini toplar, enum değerlerini okur. `std::string` (= `std::basic_string<char>`) ve `std::unique_ptr<T>` bildirimin namespace'i ve template argümanlarıyla tanınır (yalnızca ad değil).
- **Mirror**: `lower.rs:17` (`REASON_*`), `clang.rs` (`// SAFETY:`)
- **Validate**: `cargo test -p ferrobridge-parser` + `cargo insta review` (M1 snapshot farkları ve yeni 3 fixture elle incelenir)

### Task 3: Analiz: bağlanabilirlik ve adlandırma
- **Action**: `check()` IR'ı gezer; her öğe/parametre/dönüş için cxx'te güvenle ifade edilip edilemeyeceğine karar verir. Kurallar `03 Mimari` tip tablosu ve güvenlik kurallarıdır (pointer → reddet, primitif `T&` → reddet, sınıf `T&` → `Pin<&mut T>`, değerle dönen sınıf → shim'li `UniquePtr`). Bir fonksiyonun tek bir parametresi bile desteklenmiyorsa fonksiyon **tümüyle** atlanır ve neden bildirilir (yarım bağlama yok). Overload ve anahtar kelime adlandırması.
- **Mirror**: `lower.rs:17` (gerekçe metinleri), `parser/src/lib.rs:66` (hata deseni)
- **Validate**: `cargo test -p ferrobridge-analysis` (her kural için tablo testi; overload ve anahtar kelime adları)

### Task 4: Kod üretimi
- **Action**: `bridge.rs`: namespace başına `extern "C++"` blokları, `include!("shim.h")`, opak tipler, metotlar (`self: &T` / `Pin<&mut T>`), `#[cxx_name]`, `Result<T>`, extern enum'lar. `shim.h/.cc`: `ferrobridge_new_<Sınıf>_<n>`, statik metot ve değerle dönüş sarmalayıcıları; orijinal header'ı `#include` eder. Her dosyanın başında `@generated by ferrobridge — elle düzenlemeyin` ve desteklenmeyen öğeler yorum olarak.
- **Mirror**: `parser/tests/fixtures.rs:20` (snapshot deseni)
- **Validate**: `cargo test -p ferrobridge-codegen` + snapshot incelemesi; üretilen `bridge.rs`'in `syn` ile ayrıştırılabildiği test edilir

### Task 5: Uçtan uca test crate'i (`ferrobridge-e2e`)
- **Action**: `classes`, `enums`, `strings` fixture'ları için üretilmiş dosyalar `src/generated/<fixture>/`'a yazılır. `build.rs` `cxx-build` ile bunları ve fixture `impl.cc`'lerini derler. Testler Rust'tan: nesne oluşturur, metot çağırır, `const` metot ile okur, statik metodu çağırır, enum değerini karşılaştırır, exception fırlatan metottan `Err` alır, `std::string` gönderip alır.
- **Mirror**: `crates/ferrobridge-cli/tests/parse.rs` (gerçek derleme/çalıştırma)
- **Validate**: `cargo test -p ferrobridge-e2e`

### Task 6: `ferrobridge generate` komutu
- **Action**: `generate <header> --out-dir <dizin> [--include-as <yol>] [--std] [-- clang argümanları]`. Üç dosyayı yazar, atlanan her öğe için `⚠ <ad>: <neden>` basar, sonunda özet (`5 öğe bağlandı, 2 atlandı`). Hata durumunda çıkış kodu 1.
- **Mirror**: `crates/ferrobridge-cli/src/main.rs:18` ve `main.rs:78`
- **Validate**: `cargo test -p ferrobridge-cli` (dosyalar yazılıyor; çıktı e2e'deki commit'li dosyalarla birebir aynı, yani üretici ile commit'li kod senkron)

### Task 7: Güvenlik incelemesi, ADR'ler ve belgeler
- **Action**: Yeni `unsafe` blokları ve üretilen C++ shim'leri incelenir (`/ecc:security-review` + `/ecc:cpp-review`). ADR-0004 ve ADR-0005 yazılır. README'ye `generate` örneği; PRD (M2 durumu, minimum libclang 16), plan dosyası, CLAUDE.md ve Obsidian notları güncellenir.
- **Validate**: README'deki komut kopyalanıp çalıştırıldığında çıktı veriyor

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p ferrobridge-cli -- generate tests/fixtures/classes/input.h --out-dir target/gen
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| cxx'in desteklemediği bir kalıp üretilir (ör. overload'larda `cxx_name`, extern enum, statik metot) | Orta | Her kalıp önce e2e crate'inde küçük örnekle doğrulanır; desteklenmiyorsa shim'e taşınır |
| Üretilen C++ shim'i derlenmiyor ya da UB içeriyor (ör. `make_unique` ile abstract sınıf) | Orta | Abstract ve deleted constructor'lar analizde reddedilir; e2e gerçek derleme yapar; `/ecc:cpp-review` |
| `std::string`'i ad üzerinden tanımak kırılgan (libc++ `std::__1::basic_string`, libstdc++ farklı) | Yüksek | Bildirimin namespace zinciri + template argümanı (`char`) ile tanınır; macOS libc++'ta test edilir, Linux libstdc++ CI riski olarak kaydedilir |
| Scope büyüklüğü (Large) | Yüksek | `std::vector` bilinçli olarak çıkarıldı; kalıtım/operatör/field yok; gerekirse milestone 2a (sınıf+metot) / 2b (enum+string) diye bölünebilir |
| Overload adları header değişince kayar | Orta | ADR-0005'te bilinçli bedel olarak kayıt; M4 idiomatik katmanda kullanıcı ad verebilir (açıklama dosyası) |
| `-fno-exceptions` ile derlenen projelerde `Result<T>` shim'leri derlenmez | Orta (ArduPilot'ta yüksek) | M2'de exception açık varsayılır; PRD açık sorusu olarak kalır, M5'teki `AP_Math` demosundan önce karara bağlanmalı |
| `cc`/C++ derleyicisi e2e testini yavaşlatır | Düşük | Tek crate, üç fixture; CI'da önbellek |

## Acceptance
- [ ] `classes` fixture'ındaki sınıf, elle yazılmış bridge olmadan Rust testinden oluşturulup metotları çağrılıyor
- [ ] Enum ve `std::string` uçtan uca çalışıyor; `noexcept` olmayan bir metottan fırlatılan exception Rust'ta `Err` oluyor
- [ ] Desteklenmeyen her öğe/üye üretilen dosyada yorum ve CLI'da uyarı olarak görünüyor
- [ ] Kullanıcı tarafı testlerde hiç `unsafe` yok
- [ ] `generate` çıktısı e2e'deki commit'li dosyalarla birebir aynı
- [ ] Validation komutlarının hepsi geçer
- [ ] ADR-0004 ve ADR-0005 yazıldı; yeni `unsafe` blokları ve C++ shim'leri incelendi
- [ ] Patterns mirrored, not reinvented

## Uygulama notları (2026-11-27)
- **Önce cxx denemesi yapıldı** (depo dışında, geçici crate): overload'lar için `cxx_name`, extern enum'lar, `Result`, constructor/statik/string shim'leri, ayrı shim namespace'i, yeniden adlandırılmış tipler. Hepsi çalıştı. Denemede bulunanlar:
  - cxx parametre adlarını üretilen C++'a kopyalıyor; `this` gibi C++ anahtar kelimeleri kullanılamaz. Shim alıcısı `fb_self` oldu.
  - cxx'in varsayılan exception işleyicisi yalnızca `std::exception` yakalıyor; `throw 42` programı sonlandırırdı. Shim'e `rust::behavior::trycatch` eklendi, artık her exception `Err` oluyor (güvenlik kuralı 4). Birden fazla shim aynı dosyaya girerse diye `#ifndef` korumalı.
- **Görev sırası değişti:** Task 6 (`generate` komutu), Task 5'ten (e2e crate) önce yapıldı; e2e'nin commit'li dosyaları gerçek komutla üretilsin diye.
- **Homebrew libclang kaynak dizinini bulamıyordu:** `<string>`, `<memory>` içeren header'larda `stdarg.h` bulunamadı (M1'de `<cstdint>` buna ihtiyaç duymadığı için görülmemişti). Parser artık yüklenen libclang'ın yanındaki `clang/<sürüm>/` dizinini `-resource-dir` olarak veriyor. Apple libclang'da sorun yoktu.
- **Platforma bağlı tamsayılar bağlanmıyor:** cxx tipleri birebir eşleştirir; `long` (Windows'ta 32 bit), `long long` ve `size_t` platformdan platforma farklı `intN_t`'ye denk geliyor. Bunlar analizde açıklamalı olarak reddediliyor ("`std::int64_t` kullanın"). `int`, `short`, `unsigned`, `char` (→ `c_char`) ve `std::intN_t` bağlanıyor. Shim üzerinden dönüştürme sonraki bir milestone'a bırakıldı.
- **Plana ek güvenlik önlemi:** `generate`, `@generated by ferrobridge` işareti taşımayan bir dosyanın üzerine `--force` olmadan yazmıyor (review sırasında eklendi).
- **Test hatası (kodda değil):** C++ `length` fonksiyonu `float` ile hesapladığı için √5 beklentisi `f32` hassasiyetinde olmalıydı; test düzeltildi.
- **cxx sürümü:** `cargo add` 1.0.198'i seçti (workspace `rust-version = 1.85` ile uyumlu en yeni sürüm olduğu düşünülüyor; 1.0.205 denemede çalışmıştı).
- **İnceleme:** 52 `unsafe` blok (17 yeni), hepsi `clang.rs`'de ve gerekçeli. Üretilen C++'ta UB kaynağı bulunmadı. Bilinen davranışlar: kopyalanamayan/taşınamayan bir sınıf değerle dönerse shim derlenmez (hata derleme zamanında); `noexcept` shim'de bellek ayırma başarısız olursa program sonlanır (Rust'taki bellek tükenmesi gibi). `cargo audit` kurulu olmadığı için bağımlılık taraması yapılmadı.
- **Sonuç:** 65 test (IR 4, parser 14, analiz 18, codegen 7, CLI 9, e2e 13), clippy `-D warnings` ve fmt temiz.
