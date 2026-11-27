# Plan: Milestone 6: Header'lar arası tipler, template'ler ve gösterilebilir `AP_Math` demosu

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 6: Header'lar arası tipler, template'ler ve alanlar
**Girdiler**:
- `.claude/plans/m5-apmath-olcum.md` (M5 sonrası bölümü ve düzeltme notu)
- `.claude/plans/m5-derleme-hatti-ve-demo.plan.md` (uygulama notları)
- `demo/ap_math/`
**Complexity**: Large
**Durum**: Tamamlandı (2026-11-27)

## Summary
Amaç, `AP_Math` demosunu başkalarına gösterilebilir hale getirmek. Bunun için:
- `Vector2f`, `Vector3f`, `Matrix3f` ve `Quaternion` Rust'ta gerçek struct'lar olur: alanlar, metotlar ve `+ - * /` operatörleri.
- Bu tipleri kullanan başka header'lardaki fonksiyonlar da açılır (`control.h`, `location.h`, `SCurve.h`).
- Exception kullanmayan bir kütüphanede her çağrının `Result` döndürmesi gerekmez.
- Demoya bir örnek program ve "elle cxx" ile "ferrobridge" karşılaştırması eklenir.

**Ölçülebilir hedef:** `AP_Math` API'sinin **≥ %80**'i (PRD; bugün %31, aynı kaba yöntem).
- Template sınıf üyeleri, template başına bir kez sayılır (ölçümdeki gibi).
- Demo kullanıcı kodunda `unsafe` yok.
- `cargo run --example tour` çalışıyor.

## Kararlar (onayınla kabul edilir)
1. **Header'lar arası tipler: çok header'lı bridge (ADR-0009).**
   - Bir bridge birden fazla header'dan üretilir: `ferrobridge generate a.h b.h ...`, `Generate::new("ad").header(..)`.
   - Parser hepsini tek bir sentetik çeviri biriminde okur. Analiz tipleri bütün header'lar arasında tanır.
   - Bridge'e eklenmemiş bir header'daki tip yine atlanır, ama mesaj ne yapılacağını söyler: "`Rotation` `rotations.h`'ta; bu header'ı da bridge'e ekleyin".
   - Yan kazanç: tek bir `ferrobridge.toml` bütün bridge'e yeter. M5'teki "header başına açıklama dosyası" bulgusu kapanır.
   - Değerlendirilen alternatif: ayrı bridge'ler arasında tip paylaşmak (cxx `ExternType` takma adlarıyla). Kullanıcıya bridge'ler arası bağımlılık yönetimi yükler; seçilmedi.
2. **Template'ler kullanıcının listelediği somut tiplerle örneklenir (ADR-0009).**
   ```toml
   [templates]
   Vector3f = "Vector3<float>"
   Quaternion = "QuaternionT<float>"
   ```
   - Argümanlar libclang'a çözdürülür; sentetik birime `using __fb_0 = ::Vector3<float>;` eklenir.
   - Üyeler template'in kalıbından (`ClassTemplate`) okunur ve `T` somut tiple değiştirilir. libclang örneklenmiş sınıfın üyelerini göstermiyor (Task 0'da doğrulanacak).
   - C++ tarafında shim `using Vector3f = ::Vector3<float>;` takma adını tanımlar. Shim kendisi açık örnekleme **yapmaz**; kütüphanenin kendi `template class Vector3<float>;`'ıyla çakışırdı.
   - Yalnızca tip parametreli template'ler (`template <typename T>`); tip olmayan parametreler kapsam dışı.
   - Yalnızca kütüphanenin gerçekten örneklediği tipler listelenmeli. Aksi halde üyelerin tanımı bulunamaz ve bağlama hatası çıkar. README bunu söyler.
3. **Değer tipleri (ADR-0010).** Bir sınıf ya da örnekleme Rust'ta `#[repr(C)]` struct olur, şu koşulların hepsi sağlanıyorsa:
   - bütün statik olmayan alanları public
   - alan tipleri sayı, `bool` ya da başka bir değer tipi
   - sanal fonksiyon ve taban sınıf yok
   - kullanıcı tanımlı kopyalama, taşıma ve destructor yok

   Örnek: `pub struct Vector3f { pub x: f32, pub y: f32, pub z: f32 }`.
   - cxx'e `unsafe impl ExternType` (`Kind = Trivial`) ile tanıtılır. `unsafe` yalnızca üretilen iç katmanda ve `// SAFETY:` gerekçesiyle bulunur (5. kural).
   - Son sözü C++ derleyicisi söyler. `shim.cc` şunları `static_assert` ile doğrular: `is_trivially_copyable`, `is_standard_layout`, `sizeof`, `alignof` ve her alanın `offsetof`'u. Analiz yanılırsa sessiz UB değil, derleme hatası çıkar.
   - Koşulları sağlamayan sınıflar bugünkü gibi opak kalır (`UniquePtr`, `Pin`). 3. kural korunur.
   - Değer tipleri Rust'ta `Clone, Copy, Debug` türetir. `PartialEq` ve `Default` türetilmez: C++'taki `==` ve varsayılan constructor anlamını taşımayabilirler. Bunlar C++ operatöründen ya da constructor'ından bağlanır.
   - Metotlar `&self` / `&mut self` alır, `Pin` yok. Değerle dönüş doğrudan değer.
4. **Operatörler (`std::ops`).**
   - `+ - * /` (ikili), tekli `-`, `+= -= *= /=` ve `==` / `!=` → `Add`, `Sub`, `Mul`, `Div`, `Neg`, `*Assign` ve `PartialEq`.
   - Aynı operatörün farklı sağ taraf tipli overload'ları ayrı `impl`'ler olur: `Mul<f32>` ve `Mul<Vector3f>`.
   - `std::ops`'a karşılığı belirsiz olanlar (`%` = çapraz çarpım, `[]`, `^`) adlı metot olur: `op_rem`, `op_index`... Neden belgede.
   - Shim üzerinden çağrılır.
5. **Exception kullanmayan kütüphaneler (ADR-0011; PRD'deki açık soru).**
   ```toml
   [library]
   exceptions = false
   ```
   - Bu bir söz: "bu kütüphane exception fırlatmaz". Fonksiyonlar `Result` yerine doğrudan değer döndürür.
   - Söz yanlışsa program `std::terminate` ile sonlanır, tanımsız davranış olmaz. cxx'in `Result` döndürmeyen fonksiyonları `noexcept` sarmalayıp sarmalamadığı Task 0'da doğrulanacak; doğrulanmazsa bu karar düşer.
   - ferrobridge'in kendi kontrolleri (slice uzunluğu, `borrowed` null dönüşü, enum değeri) yine `Result` döner.
   - Söz, üretilen dosyaların "varsayımlar" listesine girer.
6. **Demo tek bridge olur.** Bütün `AP_Math` header'ları tek `ap_math` modülünde, tek bir `ferrobridge.toml` ile.

## Kapsam
**Var:** kararlar 1–6. Ek olarak:
- `examples/tour.rs` (Euler açılarından quaternion → vektör döndürme → matris; CRC)
- demo README'si ("5 dakikada", elle cxx ile karşılaştırma, kapsam tablosu)
- yeniden ölçüm
- ADR-0009/0010/0011

**Yok:**
- fonksiyon template'leri (`wrap_180<T>`, `is_zero<T>`): v0.2. Ölçüme göre ~%5'lik iş.
- opak sınıfların public alanları (yalnızca değer tiplerinde alan var)
- kalıtım
- tip olmayan template parametreleri (`VectorN<T, N>`)
- CMake (v0.2)
- CI ve yayın (M7)

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Template reddi | `crates/ferrobridge-parser/src/lower.rs:128` | `ClassTemplate` → `unsupported`; burası kalıp okumaya döner |
| Sınıf indirgeme | `crates/ferrobridge-parser/src/lower.rs:221` (`class`), `:677` (`lower_record`) | Örnekleme aynı yoldan; `lower_type`'a ikame ortamı eklenir |
| Sentetik birim | `crates/ferrobridge-parser/src/lib.rs:134` | `synthetic_unit`; birden fazla header + `using` satırları |
| Açıklama biçimi | `crates/ferrobridge-analysis/src/config.rs:168` | `RawFile`'a `templates`, `library` bölümleri; bilinmeyen anahtar satır numaralı hata |
| Bilinen tipler | `crates/ferrobridge-analysis/src/lib.rs:744` | `known_types`; değer tipi kararı burada |
| Extern tip | `crates/ferrobridge-codegen/src/bridge.rs:147` | `extern_type`; değer tipleri için `type X = super::X;` + `ExternType` |
| Güvenli katman | `crates/ferrobridge-codegen/src/safe.rs:280`, `:406` | `class_items`, `wrapper`; değer tiplerinde sarmalayıcı yok, struct'ın kendisi |
| Sabit doğrulama | `crates/ferrobridge-codegen/src/shim.rs` (`constant_checks`) | Düzen `static_assert`'leri aynı biçimde |
| `build.rs` | `crates/ferrobridge-build/src/lib.rs:279` | `Generate::new`; birden fazla header |
| Testler | `crates/ferrobridge-parser/tests/fixtures.rs`, `crates/ferrobridge-e2e/tests/*.rs` | Fixture + snapshot + gerçek C++ e2e; güvenli API testlerinde cxx tipi yok (`idiomatic.rs`) |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge-parser/src/{clang,lib,lower}.rs` | UPDATE | Çok header, örnekleme çözümü, kalıptan ikameli indirgeme, alanlar ve düzen bilgisi, operatörler |
| `crates/ferrobridge-ir/src/{item,types,lib}.rs` | UPDATE | IR v5: `Class.fields`, `Class.layout`, `Class.instance_of`, `Method.operator`, `SourceLocation` header'ı |
| `crates/ferrobridge-analysis/src/{config,rules,lib,plan,naming}.rs` | UPDATE | `[templates]`, `[library]`, değer tipi kararı, operatör planları, `exceptions = false` |
| `crates/ferrobridge-codegen/src/{bridge,shim,safe,spelling}.rs` | UPDATE | Değer struct'ları, `ExternType`, düzen `static_assert`'leri, `std::ops` impl'leri, takma ad `using`'leri |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | Birden fazla header, header başına `--include-as` |
| `crates/ferrobridge-build/src/lib.rs` | UPDATE | `Generate::header(..)` |
| `tests/fixtures/{multi_header,templates,value_types,operators,no_exceptions}/` | CREATE | Her karar için fixture + e2e |
| `crates/ferrobridge-e2e/**` | UPDATE | Yeni fixture'lar |
| `demo/ap_math/**` | UPDATE | Tek bridge, `ferrobridge.toml`, `examples/tour.rs`, README, testler |
| `docs/adr/0009-*`, `0010-*`, `0011-*`, `docs/adr/README.md` | CREATE / UPDATE | Kararlar 1–5 |
| PRD (iki kopya), `README.md`, `CLAUDE.md`, Obsidian, ölçüm raporu | UPDATE | |

## Tasks
Her görev TDD ile yapılır: önce başarısız test, sonra kod, sonra doğrulama.

### Task 0: Riskli varsayımları dene (repo dışında)
- **Action**:
  - (a) libclang gerçekten örneklenmiş sınıfın üyelerini göstermiyor mu? Kalıptaki `T` tipine `clang_getTypeDeclaration` ile `TemplateTypeParameter` cursor'ına ulaşılabiliyor mu? `clang_getSpecializedCursorTemplate` örneklemeden template'e götürüyor mu?
  - (b) cxx: `using Vector3f = ::Vector3<float>;` takma adı `ExternType` (`Trivial`) ile bridge'e girip şunları yapabiliyor mu?
    - değerle parametre ve dönüş
    - `self: &Vector3f` ve `self: &mut Vector3f` metotları
    - cxx'in kendi `static_assert`'leri
  - (c) cxx, `Result` döndürmeyen bir fonksiyon C++'ta exception fırlatınca ne yapıyor? `std::terminate` mu, tanımsız davranış mı?
  - (d) Bir operatörü shim üzerinden çağırıp Rust'ta `impl Add` ile sarmak.
- **Validate**: Sonuçlar plan notlarına yazılır. (c) `terminate` değilse karar 5 düşer; (b) çalışmazsa değer tipleri shim'deki serbest fonksiyonlarla yapılır.

### Task 1: Çok header'lı bridge
- **Action**:
  - Parser: `parse_headers(&[Path])`, tek sentetik birim, hedef dosya kümesi.
  - IR'daki konum header adını taşır.
  - Codegen: `header_includes: Vec<String>`.
  - CLI: birden fazla header; header başına `--include-as`.
  - `Generate::header`.
  - Bridge dışındaki header'da tanımlı tip için yeni, yol gösteren mesaj.
- **Mirror**: `lib.rs:134`, `lower.rs:68`
- **Validate**: `multi_header` fixture'ı: `a.h`'taki sınıf `b.h`'taki fonksiyonda kullanılıyor (snapshot + e2e). Mevcut snapshot'lar yalnızca IR sürümüyle değişir.

### Task 2: Template örnekleme (parser)
- **Action**:
  - `ParseOptions.instantiate: Vec<(rust_name, spelling)>`.
  - Sentetik birime `using` satırları eklenir; libclang'dan çözülmüş argümanlar ve template cursor'ı alınır.
  - Kalıbın üyeleri ikame ortamıyla indirgenir: `T`, `Vector3<T>`, `const Vector3<T>&`, başka bir örneklemeye başvuru (`Matrix3<T>` → `Matrix3f`).
  - Örneklenmemiş bir template'e başvuru `unsupported` olur; mesaj hangi örneklemeyi ekleyeceğini söyler.
  - Alanlar ve düzen, somut tipten okunur: `clang_Type_getSizeOf`, `getAlignOf`, `getOffsetOf`.
- **Mirror**: `lower.rs:221`, `:677`
- **Validate**: `templates` fixture'ı (iki parametreli template, kendine ve başka örneklemeye başvuru, örneklenmemiş template, tip olmayan parametre → `unsupported`); IR round-trip.

### Task 3: Değer tipleri (analiz + kod üretimi)
- **Action**:
  - Değer tipi kuralı (karar 3); opak sınıflara geri düşüş.
  - Bridge'de `type X = super::X;` ve `unsafe impl ExternType`, `// SAFETY:` ile.
  - Güvenli katmanda `#[repr(C)] pub struct`.
  - `shim.cc`'de düzen `static_assert`'leri.
  - Metotlar ve constructor'lar (`Vector3f::new_f32_f32_f32`).
- **Mirror**: `known_types`, `extern_type`, `constant_checks`
- **Validate**:
  - `value_types` e2e: alan okuma ve yazma, değerle dönüş, `&mut self` metodu, iç içe değer tipi (`Matrix3f` içinde `Vector3f`).
  - Kasıtlı yanlış bir düzen `static_assert` ile derleme hatası veriyor (codegen birim testi).
  - Private alanı olan sınıf opak kalıyor.

### Task 4: Operatörler
- **Action**:
  - Parser'da operatör metotları `Method.operator` ile işaretlenir; serbest operatör fonksiyonları da okunur.
  - Analiz: `std::ops` eşleme tablosu (karar 4).
  - Codegen: shim + `impl` blokları.
- **Validate**: `operators` e2e: `a + b`, `a * 2.0`, `a * b` (iç çarpım), `-a`, `a += b`, `a == b`, çapraz çarpım `op_rem`.

### Task 5: `[library] exceptions = false`
- **Action**:
  - `Annotations.library.exceptions`.
  - Analiz bütün fonksiyonları `noexcept` sayar; ferrobridge'in kendi kontrolleri `Result` olarak kalır.
  - Varsayımlar listesine söz eklenir.
- **Validate**:
  - `no_exceptions` e2e: dönüşler doğrudan değer.
  - Söz yanlışsa (C++ exception fırlatırsa) süreç sonlanıyor. Test kendini alt süreç olarak çalıştırıp anormal çıkışı doğrular.

### Task 6: Gösterilebilir demo
- **Action**:
  - Tek bridge, tek `ferrobridge.toml`. İçeriği `[templates]` (`Vector2f`, `Vector3f`, `Matrix3f`, `Quaternion`), `[library] exceptions = false` ve M5'in açıklamaları.
  - Testler yeni API'ye taşınır: `?` gürültüsü gider.
  - `examples/tour.rs`: Euler açılarından quaternion, vektör döndürme, `Matrix3f` ile karşılaştırma, CRC.
  - README: "5 dakikada çalıştır", elle yazılması gereken cxx bridge'iyle yan yana karşılaştırma (aynı `Vector3f` için), kapsam tablosu, bilinen sınırlar.
  - Yeniden ölçüm.
- **Validate**:
  - `ARDUPILOT_DIR=… cargo test` ve `cargo run --example tour` (`--manifest-path demo/ap_math/Cargo.toml`).
  - Demoda `unsafe` yok.
  - Kapsam ≥ %80.

### Task 7: İnceleme, ADR ve belgeler
- **Action**:
  - `/ecc:security-review`. Odak: `ExternType` gerekçeleri, düzen doğrulaması, ikamenin yanlış tip üretme olasılığı, `exceptions = false`.
  - `/ecc:cpp-review`.
  - ADR-0009/0010/0011.
  - PRD (iki kopya; açık soru kapanır), README, `CLAUDE.md`, Obsidian.
- **Validate**: Bütün kontroller iki libclang'da (Homebrew 23, Apple 21) geçiyor.

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
ARDUPILOT_DIR=~/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml
ARDUPILOT_DIR=~/ardupilot cargo run --manifest-path demo/ap_math/Cargo.toml --example tour
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| Kalıptan ikameli indirgeme yanlış bir tip üretir (bağımlı tipler, iç içe template'ler) | Yüksek | Yalnızca tip parametreleri. Tanınmayan her bağımlı tip `unsupported` olur, tahmin edilmez. Değer tiplerinde düzen `static_assert` ile doğrulanır. Metot imzaları C++ derleyicisince shim'de denetlenir: yanlış tip sessiz UB değil, derleme hatası |
| libclang örneklenmiş üyeleri gösteriyor ya da kalıpta beklenen bilgiyi vermiyor | Orta | Task 0 önce. Sonuca göre yöntem seçilir |
| cxx `ExternType` takma adlarıyla metot ya da değerle geçirme desteklemiyor | Orta | Task 0. Geri düşüş: değer tipleri shim'deki serbest fonksiyonlarla |
| `exceptions = false` için cxx `noexcept` sarmalamıyor | Orta | Task 0 (c). Öyleyse karar 5 düşer, `Result` kalır |
| Bir örneklemenin bazı üyeleri o `T` için derlenmiyor | Orta | Yalnızca kütüphanenin örneklediği tipler listelenir (README). Derleme hatası açık; gerekirse `ferrobridge.toml`'a üye atlama (`skip`) eklenir |
| Operatör overload'ları Rust'ta çakışır (`Mul<f32>` ile `Mul<Vector3f>` dışında) | Düşük | Çakışan operatör adlı metot olur |
| Kapsam (Large) | Yüksek | Öncelik sırası 0→1→2→3→6 (gösterilebilir demo için asgari). 4 ve 5 demonun kalitesini artırır; zaman yetmezse Task 6'dan önce ayrı karar verilir |
| %80 hedefi ölçüm yönteminin kabalığı yüzünden kayar (±%10) | Orta | Ölçüm betiği template üyelerini plandan sayacak şekilde güncellenir; yöntem raporda açıkça yazılır |

## Acceptance
- [x] Bir header'daki tip başka header'ların fonksiyonlarında kullanılabiliyor; bridge dışındaki tip için yol gösteren mesaj var
- [x] `[templates]` ile `Vector2f`, `Vector3f`, `Matrix3f`, `Quaternion` bağlanıyor
- [x] Değer tipleri Rust struct'ı (alanlar, `&mut self`); düzen C++'ta `static_assert` ile doğrulanıyor; diğer sınıflar opak
- [x] Operatörler `std::ops` ile
- [x] `exceptions = false` doğrudan değer döndürüyor; söz bozulursa süreç sonlanıyor (UB yok); ya da Task 0 sonucuyla gerekçeli olarak düşürüldü
- [x] Demo tek bridge; `examples/tour.rs` çalışıyor; README "5 dakikada" ve karşılaştırma içeriyor; demoda `unsafe` yok
- [x] `AP_Math` kapsamı ≥ %80 (yöntem raporda)
- [x] ADR-0009/0010/0011; güvenlik ve C++ incelemesi; iki libclang'da yeşil
- [x] Patterns mirrored, not reinvented

## Uygulama notları

### Task 0 (2026-11-27): bütün varsayımlar doğrulandı
- **(a) libclang:**
  - Örneklenmiş sınıfın (açık örnekleme dahil) cursor çocuğu yok. Üyeler kalıptan (`ClassTemplate`) okunmalı.
  - `clang_getSpecializedCursorTemplate` örneklemeden template'e götürüyor.
  - Argümanlar çözülüyor (`float`). Somut tipten `sizeof`, `alignof` ve `clang_Type_getOffsetOf` okunabiliyor.
  - **Plandan fark:** kalıptaki `T`'nin bildirimi bulunamıyor (`NoDeclFound`); kanonik adı `type-parameter-<derinlik>-<sıra>`. İkame bu adla yapılacak.
  - Bağımlı `const V3<T>&` gibi tipler `clang_getTypeDeclaration` ile template'e (`ClassTemplate`) ve `clang_Type_getNumTemplateArguments` ile argümanlarına ulaşıyor.
- **(b) cxx:** `using Vector3f = ::V3<float>;` takma adı `ExternType` (`Kind = Trivial`) ile bridge'e giriyor. Değerle parametre ve dönüş, `self: &Vector3f`, `self: &mut Vector3f` çalışıyor. cxx kendisi de `static_assert(IsRelocatable<…>)` üretiyor.
- **(c) cxx, `Result` döndürmeyen fonksiyonların C++ sarmalayıcısını `noexcept` yapıyor.** Exception fırlatılınca süreç `std::terminate` ile `SIGABRT` alarak sonlanıyor; tanımsız davranış yok. Karar 5 geçerli.
- **(d)** `operator+` shim üzerinden çağrılıp Rust'ta `impl Add` ile sarılabiliyor.

### Task 1 (2026-11-27): çok header'lı bridge
- **Parser:** `parse_headers(&[P])` bütün header'ları tek bir sentetik birimde, verilen sırayla include ediyor. `parse_header` artık bunun tek header'lık hali. Hedef dosyalar bir küme.
- **IR v5: `Type::Record` / `Type::Enum` isteğe bağlı bir `declared_in` alanı aldı.** Alan yalnızca tip bridge'in header'ları dışında bildirildiyse dolu ve JSON'da yalnızca o durumda görünüyor. Mevcut snapshot'larda tek fark IR sürümü. Bir istisna var: `prelude` fixture'ında `Vec2`, prelude'dan geldiği için `declared_in: "prelude.h"` aldı; bu doğru.
  - Bunu yapabilmek için `lower_type` artık bir `Types` bağlamı alıyor. Task 2'nin ikame ortamı da buraya eklenecek.
- **Analiz:** Bilinmeyen tip, bridge dışındaki bir header'daysa mesaj o header'ı adıyla söylüyor ve onu bridge'e eklemeyi öneriyor.
- **Codegen:** `CodegenOptions.header_includes: Vec<String>`. Tek header'lı çıktı birebir aynı kaldı (snapshot'lar ve e2e dosyaları değişmedi). Birden fazla header varsa başlıkta "Kaynak header'lar: …" yazıyor.
- **CLI:** `generate` ve `suggest` birden fazla header alıyor. `--include-as` header başına bir kez, aynı sırayla veriliyor; sayı tutmazsa hata.
- **`ferrobridge-build`: API değişti.** Eski: `Generate::new(header, ad).include_as(..)`. Yeni: `Generate::new(ad).header(yol)` ya da `.header_as(yol, include)`. Demo ve entegrasyon testi güncellendi.
- **e2e:**
  - Yeni kural: fixture dizinindeki bütün `.h` dosyaları (alt dizinler hariç) bridge header'ı. Bu kural CLI senkron testinde ve README'deki komutta uygulanıyor.
  - Yeni fixture `multi_header`: `geometry.h` + `input.h`, bridge dışında kalan bir `outside/units.h`.
- **Toplam:** 176 test, clippy iki modda temiz, demo 17 test.

### Task 2 (2026-11-27): template örnekleme (parser)
- **Akış:** `ferrobridge.toml` → `[templates]` (`Annotations.templates`, satır numaralı doğrulama) → CLI ve `build`, açıklamaları parse'tan önce yükleyip `ParseOptions.instantiate`'e veriyor.
- **Sentetik birim:** Her örnekleme için `using __ferrobridge_inst_N = <yazım>;` ve sınıfı örneklemeye zorlayan `static_assert(sizeof(...))` ekleniyor. Bu satırlardaki libclang hataları "`[templates]` X = "…": …" önekiyle gösteriliyor. Yazımda `; { } " #` ve satır sonu reddediliyor.
- **İkame:**
  - Kalıptaki `type-parameter-0-N` somut argümanla değiştiriliyor.
  - Bağımlı `Tmpl<T>`, eşleşen bir örnekleme varsa ona bağlanıyor.
  - Yalnızca adıyla yazılan `Vec3`: libclang bunu kalıp sınıfı (`ClassDecl`) olarak veriyor; template'le aynı ad ve konumdan tanınıyor.
  - Bunların dışındaki bağımlı tipler `Unsupported`. Üye template'lerinin parametreleri (derinlik ≠ 0) de `Unsupported`.
- **Bulunan iki sorun:**
  - (1) `const Vec3<float>` ile `Vec3<float>` tip olarak eşit değil → eşleme özelleştirme bildirimi üzerinden yapılıyor.
  - (2) İleri bildirim (`template <typename T> class Vec2;`) yüzünden template gövdesiz geliyordu → hep tanıma (`clang_getCursorDefinition`) gidiliyor; gövdesiz template bildirimleri atlanıyor.
- **IR v5'e eklenenler:**
  - `Class.fields` (erişimden bağımsız, ofset ve konumla), `Class.layout`, `Class.user_copy_or_move`, `Class.instance_of`.
  - Bit alanları `Unsupported` tip.
  - Alanlar artık parser'da "desteklenmeyen üye" değil. Analiz, opak sınıfların public alanlarını aynı gerekçeyle raporluyor; atlanan üyeler kaynak sırasıyla listeleniyor (`classes` çıktısı değişmedi).
- **Örneklenmiş template'ler** artık "desteklenmiyor" diye raporlanmıyor. Örneklenmemişlerin gerekçesi `[templates]`'i nasıl kullanacağını söylüyor.
- **ArduPilot'ta deneme:** `vector2.h`, `vector3.h`, `matrix3.h`, `quaternion.h` + `[templates]` (`Vector2f/Vector3f/Matrix3f/Quaternion`) → 4 sınıf, 120 metot. Kalan atlamalar:
  - operatörler (Task 4)
  - alanlar ve değerle sınıf parametresi (Task 3)
  - `Rotation` (bridge'e `rotations.h` eklenince açılacak)
  - `*<double>` örneklemeleri
  - `T[4]` dizisi
- **Toplam:** 181 test.

### Task 3 (2026-11-27): değer tipleri
- **Analiz:**
  - `BridgeType::Value`; `ClassPlan.value` (alanlar, bayt ofsetleri, boyut, hizalama) ve `ClassPlan.instance_spelling`.
  - Değer tipi kararı (`mark_value_types`): yapısal koşullar (polimorfik değil, taban yok, kullanıcı tanımlı kopyalama/taşıma/destructor yok, düzen biliniyor, bütün alanlar public ve bayt hizalı), ardından alan tiplerinin sabit nokta elemesi (`Line` içinde `Pt`).
  - Değer tipleri değerle parametre, değerle dönüş ve `&T`/`&mut T` olarak geçebiliyor. Constructor'ları değer döndürüyor.
  - `std::vector` / `std::unique_ptr` içinde değer tipi şimdilik reddediliyor.
  - Örneklemelerin C++ yolu `::<yazım>`.
- **Kod üretimi:**
  - Bridge'de `type X = super::X;`.
  - Örneklemeler shim namespace'inde, `using X = ::geo::V3<float>;` takma adıyla.
  - Güvenli katmanda `#[repr(C)] #[derive(Debug, Clone, Copy)] pub struct` ve `unsafe impl ::cxx::ExternType` (`Kind = Trivial`). Rust tarafında `size_of`/`align_of`/`offset_of!` const kontrolleri.
  - `shim.cc`'de `is_trivially_copyable`, `is_standard_layout`, `sizeof`/`alignof` ve alan `offsetof` `static_assert`'leri. Değer tiplerinin metotlarında `&mut self`, `Pin` yok.
- **İki `prettyplease` sınırı, biçimlendirmeden sonra metin üzerinde aşıldı:**
  - `extern` bloğunda `type X = super::X;` yazdıramıyor → işaretli `type X;` üretilip takma ada çevriliyor.
  - Yorumları atıyor → `unsafe impl`'in `// SAFETY:` yorumu sonradan ekleniyor. Workspace lint'i (`undocumented_unsafe_blocks`) bunu kabul ediyor.
- **Davranış değişikliği:** `classes` fixture'ındaki `Point { float x, y; }` artık değer tipi. Önce `UniquePtr` sarmalayıcıydı; şimdi alanları erişilebilir. Mevcut e2e testleri değişmeden geçiyor.
- **e2e `value_types` (5 test):** alanlar, `&mut self`, değerle geçirme, iç içe struct, `Vec3f` örneklemesi (cross product, `normalize`), private alanlı sınıfın opak kalması.
- **Toplam:** 190 test.
- **Test edilmeyen:** "Kasıtlı yanlış düzen derleme hatası verir" e2e ile denenmedi. `static_assert`'lerin üretildiği snapshot'ta doğrulanıyor; cxx de kendi `IsRelocatable` kontrolünü ekliyor.

### Task 4 (2026-11-27): operatörler
- **Parser:** Üye operatörler `Method.operator` (`"+"`, `"+="`…) ile normal metot.
  - Bağlanmayanlar gerekçeyle `unsupported`: atama, `new`/`delete`, `->`, `++`/`--`, `()`, `,`, tekli `&`/`*`.
  - **Düzeltilen eski hata:** Header'daki serbest `operator` fonksiyonları `operator+` adıyla geçersiz bir Rust tanımlayıcısına dönüşüp üretimi bozabiliyordu; artık gerekçeyle reddediliyor.
- **Analiz:**
  - `OperatorKind` (token + parametre sayısı), adlı metotlar (`op_add`, `op_neg`, `op_mul_assign`, `op_rem`...).
  - Overload'lar mevcut kuralla sonek alıyor (`op_mul_f32`, `op_mul_V2`).
  - Operatör metotları her zaman shim üzerinden; shim `fb_self.operator+(v)` diye çağırıyor.
- **Kod üretimi:** Değer tiplerinde hata dönmeyen operatörler için `std::ops` impl'leri: `Add/Sub/Mul/Div/Rem/BitXor/BitAnd/BitOr<Rhs>`, `Neg`/`Not`, `*Assign<Rhs>`, `PartialEq` (yalnızca `Self` ile, `bool` dönüşlü).
  - Sağ taraf sayı, değer tipi ya da `const&` değer tipi olabilir.
  - Aynı trait ve sağ taraf ikinci kez üretilmiyor.
  - Karşılaştırmalar (`<`…) ve `[]` adlı metot kalıyor: `PartialOrd`'un anlamı C++'takiyle örtüşmeyebilir.
  - Opak sınıflarda yalnızca adlı metot var.
- **Davranış değişikliği:** `classes` fixture'ındaki `Vehicle::operator==` artık `op_eq` metodu.
- **e2e `operators` (4 test):** `a + b`, `b - a`, `-a`, `a * 2.0`, `a * b` (iç çarpım), `a % b`, `a += b`, `==`/`!=`, `op_lt`; `noexcept` olmayan `op_div` `Result` dönüyor (sıfıra bölmede `Err`); opak `Counter::op_add_assign`.
- **Toplam:** 195 test.

### Task 5 (2026-11-27): `[library] exceptions = false`
- **Config:** `[library]` bölümü; bilinmeyen anahtar satır numaralı hata veriyor. `exceptions = false` ise `Annotations.no_exceptions` sözün satırını tutuyor.
- **Analiz:** Söz verildiyse bütün fonksiyonlar `noexcept` sayılıyor. Slice uzunluk kontrolü ve `borrowed` null dönüşü gibi ferrobridge'in kendi kontrolleri `Result` olarak kalıyor. Söz "varsayımlar" listesine giriyor: `(kütüphane) exception'lar …`.
- **Kod üretimi:** Değişiklik gerekmedi. cxx `Result` dönmeyen fonksiyonları `noexcept` sarmalıyor (Task 0); shim de `noexcept`.
- **Ek düzeltme:** Değer tipleri için "public constructor yok; yalnızca başka fonksiyonların döndürdüğü nesnelerle kullanılabilir" notu artık basılmıyor; değer tipleri alanlarından kurulabiliyor.
- **e2e `no_exceptions` (3 test):**
  - Doğrudan değer dönüşü.
  - `noexcept` yazılmamış bir operatör artık trait oluyor.
  - Slice fonksiyonu hâlâ `Result` dönüyor.
  - Sözü bozan bir çağrı, alt süreçte süreci sonlandırıyor (stderr'de C++ exception mesajı görünüyor).
- **Toplam:** 201 test. PRD'deki `-fno-exceptions` açık sorusunun cevabı bu söz oldu (ADR-0011'de yazılacak).

### Task 6 (2026-11-27): gösterilebilir demo
- **Tek bridge:** Demo artık bütün `AP_Math/*.h` (21 header) için tek bir `ap_math` bridge'i. `lib.rs` bridge'i crate köküne açıyor: `ferrobridge_demo_ap_math::{Vector3f, crc_crc32}`.
- **Tek `ferrobridge.toml`:** M5'in dört açıklama dosyası birleşti. Ek olarak:
  - `[library] exceptions = false`. Dayanağı doğrulandı: waf `-fno-exceptions`, `AP_Math`'te `throw` yok.
  - `[templates]` (`Vector2f/3f`, `Matrix3f`, `Quaternion` ve `double` sürümleri). Hepsi `.cpp`'lerde açıkça örnekleniyor.
  - `to_euler` çıkış parametreleri (kaynak okunarak).
- **İlk derleme sorunsuz geçti:** 12 sınıf (8'i değer tipi), 513 fonksiyon/metot, 41 `std::ops` impl'i, 48 düzen `static_assert`'i.
- **Güvenlik bulgusu → plandan sapma.** `operator[]`'nin `const` hali açıklama gerektirmeden `op_index` olarak bağlanıyordu. ArduPilot'ta `[]` sınır denetimi yapmıyor (`MATH_CHECK_INDEXES` kapalı); güvenli Rust'tan `v.op_index(5)` tanımsız davranış olurdu. `operator[]` artık hiç bağlanmıyor; gerekçesi değer tiplerinde alanları önermek. `operators` fixture'ına da eklendi. Planın 4. kararındaki "`[]` adlı metot olur" maddesi bu yüzden düştü.
- **Testler:** `?`'siz yeni API'ye taşındı; 14 test. Kapsananlar:
  - `Vector3f` operatörleri, iç ve çapraz çarpım
  - quaternion ile matris döndürmesinin aynı sonucu vermesi
  - `Rotation` ile `rotate`
  - `to_euler`
  - CRC katalog değerleri
  - slice taşması `Err`
  - `SCurve`, sabitler
- **`examples/tour.rs`:** vektörler, yönelim, sensör montajı, kontrolcü, CRC, sabitler. `cargo run --example tour` çalışıyor.
- **README:** "5 dakikada", elle cxx ile karşılaştırma (59 açıklama satırı → 8 000+ satır üretilmiş kod), kapsam, bilinen sınırlar. Sınırlar arasında şu da yazılı: ferrobridge C++ ön koşullarını bilemez.
- **Küçük düzeltmeler:**
  - `ferrobridge-build`'in özet uyarısı birden fazla header'da 21 yolu tek satırda sıralıyordu; artık "`ap_math` (21 header)".
  - Slice taşma mesajı öğe adını iki kez basıyordu (`crc_crc8: crc_crc8: …`); `ferrobridge::Error` zaten ekliyor.
- **Ölçüm:** demonun yapılandırmasıyla **%81,8** (337/412; template üyeleri bir kez sayılarak), alanlarla %82; yalnızca `float` ile %78. Hedef (≥ %80) tuttu. Ayrıntı: `m5-apmath-olcum.md` → "M6 sonrası".
- **Bilinen sınır:** Overload adları uzun (`from_euler_f32_f32_f32`; ADR-0005). README'de yazıldı.

### Task 7 (2026-11-27): inceleme, ADR'ler, belgeler
- **İnceleme.** Odak: `ExternType` gerekçeleri, düzen doğrulaması, ikame, operatörler, `exceptions = false`. Düzeltilen iki bulgu:
  1. **(Güvenlik)** `const` alanlı sınıf (`struct P { const float x; }`) bit bit kopyalanabildiği için değer tipi sayılıyordu. Rust'tan `p.x = …` yazmak C++'ta `const` nesneyi değiştirmek olurdu → `const` alanlı sınıflar opak (analiz testi + e2e `Frozen`).
  2. **(C++)** `offsetof(::geo::Pair<int, float>, first)`: makro, yazımdaki virgülde argümanları bölüyordu ve derleme hatası çıkıyordu → örneklemelerde `static_assert`'ler shim'deki takma adı kullanıyor (e2e `PairIF`).
- **Sorunsuz bulunanlar:**
  - Yanlış bir ikame, shim'in ve cxx'in tipli C++ imzalarında derleme hatası olur.
  - `Copy`, `is_trivially_copyable` ile örtüşüyor; bu, trivial destructor'ı da kapsıyor.
  - `PartialEq` (`Eq` değil) float NaN'larıyla uyumlu.
  - `operator[]` zaten Task 6'da kaldırılmıştı.
- **ADR'ler:** ADR-0009 (çok header + template örnekleme), ADR-0010 (değer tipleri, operatörler, `operator[]`), ADR-0011 (`exceptions = false`).
- **Belgeler:**
  - Kök README: çok header, yeni tip tablosu satırları, `[templates]`/`[library]` örnekleri, "ön koşulları bilemez" uyarısı.
  - `CLAUDE.md`, PRD (iki kopya; M6 complete, `-fno-exceptions` açık sorusu kapandı), Obsidian (00/02/03/04).
- **Son doğrulama:** fmt, clippy (iki mod), `cargo doc -D warnings`; 203 test Homebrew 23 ve Apple 21 libclang'da aynı; demo 14 test ve `cargo run --example tour`.
