# Plan: Milestone 4 — Güvenli API

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 4 — Güvenli API ("Kullanıcı hiç `unsafe` yazmadan C++ nesnesi oluşturup kullanabilir; exception'lar `Result` olarak gelir")
**Complexity**: Large
**Durum**: Tamamlandı (2026-11-27, 5 kararla birlikte onaylandı)

## Summary
Bugün üretilen kod güvenli ama **idiomatik değil**. Kullanıcı `Vehicle_new_i32(4)`, `vehicle.pin_mut().drive(..)`, `let_cxx_string!`, `CxxVector` ve `cxx::Exception` ile uğraşıyor. Bu milestone'da `bridge.rs`, cxx katmanının (`ffi`) üstüne **güvenli bir Rust katmanı** da üretir. Bu katmanda `Vehicle::new(4)?`, `vehicle.drive(12.5)?`, `&str` / `String`, `&[T]` / `Vec<T>`, gerçek Rust `enum`'ları ve tek bir hata tipi `ferrobridge::Error` var. `03 Mimari` tip tablosunun "Güvenli API" sütunu bu milestone'da hayata geçiyor.

Kullanım örneği (hedef):
```rust
use mylib::{Vehicle, greet, Mode};

let mut vehicle = Vehicle::new_i32(4)?;     // C++ constructor → Result<Vehicle, ferrobridge::Error>
vehicle.drive(12.5)?;                      // &mut self, Pin yok
let km = vehicle.odometer();               // noexcept → doğrudan değer
let text: String = greet("Ada")?;          // &str girer, String çıkar
let next: Mode = mylib::next(Mode::Auto)?; // gerçek Rust enum'u, `match` tam kapsamlı
```

## Tasarım (onayla kabul edilecek kararlar)
1. **İki katmanlı güvenli API.**
   - Güvenli metotlar cxx'in opak tipine inherent `impl ffi::Widget { .. }` olarak üretilir (`&self` / `self: Pin<&mut Self>`). Böylece sahip olunan nesnede, `&ffi::Widget`'ta ve M3'ün ödünç dönüşlerinden gelen `Pin<&mut ffi::Widget>`'ta aynı API çalışır.
   - Üstüne ince bir **sahip olan sarmalayıcı** gelir: `pub struct Widget { inner: UniquePtr<ffi::Widget> }`. `Deref<Target = ffi::Widget>` ile const metotlara erişir. Değiştiren metotları `&mut self` ile iletir, `Pin` kullanıcıdan gizlenir. Constructor'lar `Widget::new*`, statik metotlar `Widget::max_wheels()` olur.
   - Bedeli: cxx'in kendi metot adları çakışmasın diye bridge'de `raw_<ad>` olur (`#[cxx_name]` C++ adını korur). `ffi` katmanı kaçış kapısı olarak `pub` kalır.
   - Sarmalayıcının değişmezi "içindeki `UniquePtr` null değil": null gelirse sarmalayıcı hiç oluşturulmaz (bkz. 4).
2. **Çalışma zamanı crate'i `ferrobridge`** (`crates/ferrobridge`, yeni). İçinde `Error`, `Result<T>` ve üretilen kodun kullandığı küçük yardımcılar var (`#[doc(hidden)] pub mod __private`). Bağımlılığı yalnızca `cxx`, `#![forbid(unsafe_code)]`. Kullanıcı crate'i bunu bağımlılık olarak ekler. Alternatif olarak her header'a ayrı bir `Error` üretilebilirdi; ama o zaman iki header'ın hataları farklı tip olurdu ve `?` ile birleşmezdi.
3. **Güvenli katman aynı `bridge.rs` dosyasında**, `pub mod ffi`'nin yanında. Çıktı dosyası sayısı ve `#[path]` ile dahil etme değişmez (ADR-0004).
4. **Tip eşlemesi (güvenli katman):**

| cxx katmanı | Güvenli katman | Not |
|---|---|---|
| `Result<T>` (`cxx::Exception`) | `Result<T, ferrobridge::Error>` | Hata hangi C++ öğesinden geldiğini de taşır (`own::Widget::adopt: ...`) |
| `&CxxString` parametresi | `&str` | Shim yok: `let_cxx_string!` üretilen kodda |
| `UniquePtr<CxxString>` dönüşü | `String` | Geçersiz UTF-8 → `Error::Utf8` |
| `&CxxString` dönüşü (metot) | `Result<&str>` | Ömür alıcıya bağlı |
| `&CxxVector<T>` parametresi (T: sayı) | `&[T]` | Her çağrıda kopyalanır (belgelenir) |
| `&CxxVector<CxxString>` parametresi | `&[&str]` | |
| `UniquePtr<CxxVector<T>>` dönüşü (T: sayı / `CxxString`) | `Vec<T>` / `Vec<String>` | |
| Constructor / değerle dönen sınıf (`UniquePtr<T>`, null olamaz) | `T` (sarmalayıcı) | Null gelirse `Error::NullPointer` (savunma amaçlı) |
| `owned` pointer / `std::unique_ptr<T>` dönüşü (null olabilir) | `Option<T>` | |
| `UniquePtr<T>` parametresi (sahiplik C++'a geçer) | `T` (değerle) | |
| shared enum | gerçek `pub enum` | C++'tan gelen değer tanımsızsa `Error::UnknownEnumValue`; bu yüzden enum döndüren fonksiyon `noexcept` olsa bile `Result` döner |
| `&T`, `Pin<&mut T>`, `&mut f64`, `Pin<&mut CxxString>`, `Pin<&mut CxxVector<T>>`, sınıf elemanlı `CxxVector` | **olduğu gibi** | Değiştirilebilir string/vector için kopyala-geri yaz yapılmaz: C++ referansı saklıyorsa kopya sessizce eskir |

5. **Thread güvenliği varsayılmaz.** cxx'in opak tipleri `!Send` / `!Sync`. Sarmalayıcılar da öyle kalır. C++ sınıfının thread-safe olduğu bir açıklama olmadan bilinemez (v0.2: `ferrobridge.toml`'da `send = true`).

## Kapsam
**Var:** yukarıdaki 5 karar; her fixture için güvenli katman; güvenli katmanı kullanan e2e testleri; ADR-0007.

**Yok (gerekçeli):**
- `Clone` (kopya constructor'ı): IR kopya constructor'ını kaydetmiyor; v0.2.
- Kalıtım → trait, operatörler → `std::ops`: PRD kapsam dışı (v0.2).
- Sınıf elemanlı vector'leri `Vec<T>`'ye çevirmek: elemanlar opak, değerle tutulamaz (güvenlik kuralı 3).
- Null olabilen parametreler (`Option<&T>`), pointer + uzunluk → slice: M3'ten devreden, v0.2.
- `ferrobridge-build` (`build.rs` yardımcısı): M5.

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Bridge üretimi | `crates/ferrobridge-codegen/src/bridge.rs:16` | `quote!` → `syn::File` → `prettyplease`; namespace blokları |
| Metot bildirimi | `crates/ferrobridge-codegen/src/bridge.rs:168` | `function_decl`: alıcı, parametreler, `Result` sarmalaması, `'a` ömrü |
| Enum | `crates/ferrobridge-codegen/src/bridge.rs:107` | `shared_enum`: değerler C++ ile derleme zamanında doğrulanır |
| Tip yazımı | `crates/ferrobridge-codegen/src/spelling.rs:16` | `rust_type` / `cpp_type` birebir; güvenli katman için yeni `safe_type` aynı desende |
| Adlandırma | `crates/ferrobridge-analysis/src/naming.rs:156`, `:194` | ADR-0005; güvenli adlar bunlardan türetilir (`Vehicle_new_i32` → `Vehicle::new_i32`) |
| Hatalar | `crates/ferrobridge-parser/src/lib.rs:66` | `thiserror` enum'u, Türkçe mesaj |
| Tests | `crates/ferrobridge-codegen/tests/fixtures.rs`, `crates/ferrobridge-e2e/tests/ownership.rs` | Snapshot + gerçek C++ ile e2e; sayaçlı testlerde kilit |
| Logging | — | Yok; CLI çıktısı değişmez |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge/{Cargo.toml,src/lib.rs}` | CREATE | `Error`, `Result`, `__private` yardımcıları |
| `Cargo.toml` (kök) | UPDATE | Yeni crate, bağımlılık grafiği yorumu |
| `crates/ferrobridge-cli/Cargo.toml` | UPDATE | Bin hedefi `doc = false` (`ferrobridge` kütüphanesiyle doküman adı çakışmasın) |
| `crates/ferrobridge-codegen/src/safe.rs` | CREATE | Güvenli katman üretimi: enum'lar, sarmalayıcılar, `impl ffi::T`, serbest fonksiyonlar, dönüşümler |
| `crates/ferrobridge-codegen/src/{bridge,spelling,lib}.rs` | UPDATE | Ham metotlar `raw_*`; güvenli katmanı dosyaya ekleme |
| `crates/ferrobridge-codegen/tests/snapshots/*` | UPDATE | Bütün `bridge.rs` snapshot'ları |
| `crates/ferrobridge-e2e/{Cargo.toml,src/generated/**}` | UPDATE | `ferrobridge` bağımlılığı, yeniden üretim |
| `crates/ferrobridge-e2e/tests/*.rs` | UPDATE / CREATE | Testler güvenli API'ye taşınır; her fixture'da ham katmanı kullanan tek bir "kaçış kapısı" testi kalır; `idiomatic.rs`: test kaynaklarında `pin_mut`, `let_cxx_string`, `cxx::`, `unsafe` geçmiyor |
| `docs/adr/0007-*.md`, `docs/adr/README.md` | CREATE / UPDATE | Kararlar 1–3 |
| `README.md`, PRD, `CLAUDE.md`, Obsidian | UPDATE | Kullanım örnekleri güvenli API'yle; M4 durumu |

## Tasks
Her görev TDD ile yapılır: önce başarısız test, sonra kod, sonra doğrulama. Önce repo dışında kısa bir **cxx denemesi** (M2 ve M3'teki gibi). Denenecekler:
- Opak tipe inherent `impl` ve `#[cxx_name]` ile yeniden adlandırılmış metot
- Shared enum'un Rust tarafındaki gerçek biçimi (cxx onu `struct` + sabitler olarak mı üretiyor?)
- `CxxVector::new()` + slice'tan doldurma
- `UniquePtr` alanlı sarmalayıcıda `Deref`
- Üretilen kodun workspace lint'leriyle (`clippy -D warnings`, `missing_docs`) temiz olması

### Task 1: Çalışma zamanı crate'i `ferrobridge`
- **Action**: `Error` (`Cpp { item, message }`, `Utf8 { item }`, `NullPointer { item }`, `UnknownEnumValue { ty, value }`), `Result<T>`, `Display` ("own::Widget::adopt: motor arızası"), `std::error::Error`. `__private` modülü: `cpp_error(item) -> impl FnOnce(cxx::Exception) -> Error`, `string_from_cxx`, `vec_from_cxx`, `cxx_vector_from_slice`, `strings_from_cxx`.
- **Mirror**: `parser/src/lib.rs:66`
- **Validate**: `cargo test -p ferrobridge`

### Task 2: Ham katmanı yeniden adlandırma
- **Action**: Bridge'deki metotlar `raw_<ad>` + `#[cxx_name = "<C++ adı>"]`. Shim'ler ve serbest fonksiyonlar `ffi` içinde zaten çakışmıyor, onlar değişmez.
- **Mirror**: `bridge.rs:168`, `cxx_name_attr`
- **Validate**: `cargo test -p ferrobridge-codegen` + snapshot incelemesi; e2e testleri yeni adlarla derleniyor

### Task 3: Güvenli tip eşlemesi (`safe.rs`)
- **Action**: `BridgeType` / `Returns` → güvenli Rust tipi + iki yöndeki dönüşüm ifadesi (bkz. karar 4). Tablo testleri: her satır için üretilen imza ve dönüşüm.
- **Mirror**: `spelling.rs:16`
- **Validate**: `cargo test -p ferrobridge-codegen safe::`

### Task 4: Güvenli katman üretimi
- **Action**:
  - Gerçek `pub enum`'lar, `From<Enum> for ffi::Enum` ve `TryFrom<ffi::Enum> for Enum`.
  - Her sınıf için sarmalayıcı `struct`: `Deref`, `pin_mut()`, `into_inner()`, `from_unique_ptr() -> Option<Self>`, constructor'lar ve statik metotlar.
  - `impl ffi::Sınıf` içinde güvenli metotlar; değiştiren metotlar için `&mut self` iletici.
  - Modül seviyesinde güvenli serbest fonksiyonlar.
  - Her öğeye kaynağını söyleyen bir doküman yorumu (`/// C++: \`own::Widget::adopt\``).
  - Ad çakışması: sarmalayıcı `struct` adlı alanlı tanımlanır (tuple struct değil), böylece değer ad alanında bir fonksiyonla çakışmaz.
- **Mirror**: `bridge.rs:16`, `:79`
- **Validate**: `cargo test -p ferrobridge-codegen` + snapshot incelemesi; üretilen dosya `syn` ile ayrıştırılıyor

### Task 5: Uçtan uca testler
- **Action**:
  - Mevcut e2e testleri güvenli API'ye taşınır (classes, enums, strings, ownership, vectors).
  - Her fixture'da ham katmanı kullanan bir "kaçış kapısı" testi kalır.
  - Yeni testler: geçersiz UTF-8 döndüren C++ fonksiyonu → `Error::Utf8`; tanımsız enum değeri döndüren fonksiyon → `Error::UnknownEnumValue`; hata mesajında C++ öğesinin adı.
  - `idiomatic.rs`: güvenli API testlerinde `pin_mut`, `let_cxx_string`, `CxxString`, `cxx::`, `unsafe` geçmiyor.
- **Mirror**: `crates/ferrobridge-e2e/tests/ownership.rs`
- **Validate**: `cargo test -p ferrobridge-e2e`

### Task 6: İnceleme, ADR ve belgeler
- **Action**:
  - `/ecc:rust-review`. Odak: sarmalayıcının null olmama değişmezi, `Deref` ve `Pin` doğruluğu, ömürler, gizli kopyalamanın maliyeti.
  - ADR-0007.
  - README'nin kullanım bölümü güvenli API'yle yeniden yazılır.
  - PRD, plan notları, CLAUDE.md, Obsidian.
- **Validate**: README örneği e2e testlerinden birinin kopyası olarak derleniyor

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps   # bin/lib doküman adı çakışması yok
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| cxx opak tipe inherent `impl`'e izin vermiyor ya da `#[cxx_name]` ile yeniden adlandırma metotlarda çalışmıyor | Düşük | Önce cxx denemesi. Olmazsa yedek tasarım: güvenli metotlar yalnızca sarmalayıcıda, ödünç nesneler için `WidgetRef<'a>` / `WidgetMut<'a>` görünüm tipleri |
| Üretilen güvenli kod workspace lint'lerine takılıyor (`missing_docs`, `clippy::all`) | Orta | Her öğeye doküman yorumu; gerekirse yalnızca belirli lint'lere modül seviyesinde `allow`, gerekçeli |
| `ferrobridge` (kütüphane) ile `ferrobridge` (CLI binary) doküman/çıktı adı çakışması | Orta | CLI bin hedefinde `doc = false`; `cargo doc` doğrulamada |
| Gizli kopyalar (`&[T]` → `CxxVector`, `String` dönüşleri) büyük verilerde yavaş | Orta | Belgelenir; ham `ffi` katmanı kopyasız kullanım için açık kalır |
| Bir enum'u döndüren `noexcept` fonksiyonun `Result` dönmesi şaşırtıcı | Düşük | Doküman yorumunda neden yazar; alternatifi C++'tan gelen tanımsız değerde panik ya da sessiz kırpma, ikisi de kabul edilemez |
| Ham metotların `raw_*` olarak yeniden adlandırılması M2/M3 kullanıcılarını kırar | Düşük | v0.1 öncesi, dış kullanıcı yok |
| Kapsam büyüklüğü (Large) | Orta | Kopya constructor'ı, kalıtım, değiştirilebilir string/vector dönüşümleri bilinçli olarak dışarıda |

## Acceptance
- [x] Bütün fixture'lar için güvenli katman üretiliyor; `ffi` katmanı kaçış kapısı olarak duruyor
- [x] E2e testleri güvenli API'yle yazılı; test kaynaklarında `unsafe`, `pin_mut`, `let_cxx_string`, `cxx::` yok
- [x] Bütün C++ exception'ları `ferrobridge::Error` olarak geliyor ve hangi C++ öğesinden geldiğini söylüyor
- [x] Geçersiz UTF-8 ve tanımsız enum değeri panik değil `Err`
- [x] Sahip olan sarmalayıcılar asla null değil; null olabilen dönüşler `Option`
- [x] `cargo doc` ve bütün validation komutları geçiyor
- [x] ADR-0007 yazıldı; Rust incelemesi yapıldı
- [x] Patterns mirrored, not reinvented

## Uygulama notları
### cxx denemesi (2026-11-27, cxx 1.0.205, repo dışında)
Hepsi çalıştı:
- Opak tipe inherent `impl ffi::Widget`.
- `#[cxx_name = "value"] fn raw_value(self: &Widget)` ile metot yeniden adlandırma.
- `Deref<Target = ffi::Widget>` ile sahip olan sarmalayıcı.
- `CxxVector::<i32>::new()` + `push`.
- `ffi::Widget_new(..)` sonucunu sarmalayıcıya koyma.
- Deneme crate'inde `clippy -D warnings` ve `missing_docs` temiz.

**Bulgu 1:** cxx shared enum'u Rust'ta gerçek bir `enum` değil, `#[repr(transparent)] struct { repr }` + sabitler olarak üretiyor. `match`'te `other => other.repr` ile tanımsız değer yakalanabiliyor. Gerçek Rust `enum`'una `TryFrom` ile çevirmek bu yüzden gerekli ve mümkün.

**Bulgu 2 (sapma):** `CxxVector<CxxString>`'e Rust'tan eleman eklenemiyor (`push` yalnızca trivial tipler için var). Karar 4'teki `&CxxVector<CxxString>` → `&[&str]` satırı kaldırıldı; string vector parametreleri ham `&CxxVector<CxxString>` olarak kalıyor. String vector **dönüşleri** `Vec<String>` olmaya devam ediyor.

### Sonuç (2026-11-27)
Bütün görevler tamamlandı. Toplam 134 test: IR 6, parser 11, analiz 48, codegen 12, CLI 13 + parse, çalışma zamanı 5 + doküman testi, e2e 33 (güvenli API, ham katman, README örneği, idiomatiklik denetimi). Homebrew libclang 23.1.6 ve Apple libclang 21.0.0 aynı sonucu veriyor. `fmt`, `clippy -D warnings` ve `cargo doc` temiz.

### Sapmalar ve bulgular
- **`&[&str]` girişi yok** (bkz. bulgu 2): string vector parametreleri ham `&CxxVector<CxxString>` olarak kalıyor.
- **Ömürler hiç yazılmıyor.** `self: Pin<&mut Self>` alan metotlarda da çıkarım çalışıyor; açık `'a` clippy `needless_lifetimes`'a takılıyordu. Ham shim katmanındaki açık `'a` (M3) değişmedi.
- **`cargo fmt` yine üretilmiş dosyaları biçimlendiriyordu.** M3'te yalnızca `ffi` modülü `#[rustfmt::skip]` taşıyordu. Artık güvenli katmandaki her üst düzey öğe de taşıyor.
- **Lint'ler:** C++ adları korunduğu için her güvenli öğe dar bir `#[allow(non_snake_case, non_camel_case_types, clippy::should_implement_trait, clippy::too_many_arguments)]` taşıyor. Dosya seviyesinde inner attribute kullanılmadı; `include!` ile dahil eden kullanıcıları bozmasın.
- **Deref coercion sınırı:** Sarmalayıcıyı `&ffi::T` bekleyen bir fonksiyona `f(&g()?)` biçiminde vermek derlenmiyor, çünkü `?`'in tipi bağlamdan çıkarılamıyor. Değer önce bir değişkene bağlanmalı. README ve ADR-0007'de belgelendi.
- **Sarmalayıcı yalnızca bridge'de `UniquePtr<T>` olarak geçen sınıflar için üretiliyor.** cxx, `UniquePtr<T>` için gereken C++ kodunu yalnızca bridge'de geçen tipler için üretir. Yalnızca referansla görülen sınıfların sarmalayıcısı yok; güvenli metotları `ffi::T` üzerinde, statik metotları `ffi::T::ad()` olarak.
- **Fixture eklemeleri:** `text::raw_bytes` (geçersiz UTF-8) ve `io::broken_mode` (tanımsız enum değeri). İki hata yolu da gerçek C++ ile sınanıyor.
- **E2e düzeni:** Güvenli API testleri fixture başına bir dosyada. Ham katman `raw_layer.rs`'te. `idiomatic.rs` diğer test dosyalarında `unsafe`, `let_cxx_string`, `CxxString`, `CxxVector`, `cxx::`, `ffi::`, `.raw_` geçmediğini denetliyor; mutasyon denemesiyle doğrulandı. `readme.rs` README örneğinin derlenen kopyası.
- **TDD sırası:** `safe.rs`'in tablo testleri kod yazıldıktan sonra eklendi. Davranış öncesinde e2e derlemesi ve clippy ile sınanmıştı.
- **Açık kalanlar:** `Clone` (kopya constructor'ı), kalıtım → trait, değiştirilebilir string/vector dönüşümleri, sınıf elemanlı vector'ler, `Send`/`Sync` açıklaması (v0.2). `-fno-exceptions` sorusu M5'ten önce karara bağlanmalı.
