# Plan: Milestone 5 — Derleme hattı ve `AP_Math` demo iskeleti

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 5 — Derleme hattı ve `AP_Math` demosu (template'siz). Ölçüm raporundaki seçenek 2: eski M5 ikiye bölündü, template'ler M6'ya kaldı.
**Girdi**: `.claude/plans/m5-apmath-olcum.md` (bugünkü kapsam ~%20; template olmayan API'de ~%59)
**Complexity**: Large
**Durum**: Tamamlandı (2026-11-27)

## Summary
ferrobridge'i ilk kez gerçek bir kütüphane üzerinde, gerçek bir `build.rs` hattıyla çalıştırıyoruz. `ferrobridge-build` crate'i `build.rs` içinden kullanılabilir hale gelir. Ölçümün bulduğu araç sınırları kapatılır:
- kendini include eden header'lar
- `static inline` fonksiyonlar
- sabitler
- pointer + uzunluk → slice

`demo/ap_math/` crate'i gerçek bir ArduPilot checkout'una karşı derlenir. Template olmayan `AP_Math` API'si (`crc_*`, `control.h`, `location.h`, `SCurve`, `SplineCurve`, açı dönüşümleri, sabitler, `Rotation` enum'u) Rust'tan `unsafe` olmadan çağrılır.

**Ölçülebilir hedef:** template olmayan `AP_Math` API'sinin **≥ %85**'i (bugün %59), toplamda **≥ %30** (bugün %20). PRD'nin ≥ %80 hedefi M6'ya (template'ler ve alanlar) taşınır.

## Kararlar (onayınla kabul edilir)
1. **Milestone bölünmesi.**
   - M5 = derleme hattı + demo iskeleti (bu plan).
   - M6 = template örnekleme, public alanlar ve `AP_Math` ≥ %80. Genel template desteği PRD'de MVP dışındaydı; M6'nın planında ADR ile MVP'ye alınacak.
   - M7 = v0.1 yayını.
   - PRD tablosu ve Obsidian buna göre güncellenir.
2. **Demo'nun lisansı ve yeri (ADR-0008).**
   - Demo bu repoda `demo/ap_math/` altında, **ayrı bir Cargo workspace'i**. `cargo test --workspace` ArduPilot istemez; CI'da ayrı iş olarak çalışır (`02 Planlama`'daki karar).
   - ArduPilot kodu repoya girmez; demo `ARDUPILOT_DIR` ortam değişkeniyle bir checkout'a işaret eder.
   - GPL header'larından türeyen **üretilmiş dosyalar commit'lenmez**, derleme sırasında `OUT_DIR`'e üretilir. Böylece repo GPL'den türemiş metin taşımaz. Bu, ADR-0004'e bilinçli bir istisna; ADR-0008'de gerekçelenir.
   - Demo'nun kendi Rust kodu GPL kütüphanesine bağlandığı için `demo/ap_math/` dizini **`GPL-3.0-or-later`** lisanslı olur (kendi `LICENSE` dosyası, `publish = false`). Ana crate'ler `MIT OR Apache-2.0` kalır.
3. **`ferrobridge-build`'in iki modu.**
   - **Varsayılan:** commit'li üretilmiş dosyaları derler. libclang gerekmez; ADR-0004'teki kullanıcı akışı.
   - **`generate` özelliği:** `build.rs` içinde header'ı okur, `OUT_DIR`'e üretir ve derler. libclang gerekir; demo bunu kullanır.
   - Builder API: `Bridge::new(header).include_dir(..).define(..).prelude(..).config(..).sources(..).compile(name)`.
4. **Slice açıklaması** (M3'te v0.2'ye ertelenmişti, ölçüm nedeniyle öne alındı):
   ```toml
   [functions."crc_crc8".params]
   p = { slice = "len" }   # p, len elemanlık bir diziyi gösteriyor; çağrı süresince kullanılıyor
   ```
   - `const T* p, N len` → Rust'ta tek parametre `&[T]`; `T* p` → `&mut [T]`. `len` Rust imzasından kalkar.
   - T yalnızca sayı tipi olabilir. `N` herhangi bir tamsayı tipi olabilir, `size_t` dahil, çünkü Rust'a hiç görünmez.
   - Shim uzunluğu `N`'ye sığıyor mu diye kontrol eder; sığmıyorsa exception → `Err`. Sessiz kırpma yok.
   - Söz varsayımlar listesine girer ("`len` elemanlık dizi; saklanmıyor").
5. **`static inline` fonksiyonlar**, yalnızca **tanımları header'da görünüyorsa** bağlanır. cxx'in ve shim'in C++ dosyası header'ı include ettiği için fonksiyon o çeviri biriminde tanımlıdır. Tanımı olmayan iç bağlantılı bildirimler reddedilmeye devam eder (aksi halde bağlama hatası çıkar).
6. **Sabitler:**
   - `const`/`constexpr` ilkel tipli değişkenlerin değeri libclang ile hesaplanır (`clang_Cursor_Evaluate`). Güvenli katmanda `pub const WGS84_A: f64 = 6378137.0;` olarak üretilir.
   - Sonlu olmayan değerler (`NaNf`, `inf`) açıklamalı olarak reddedilir: `f32::NAN` NaN'ın payload'ını korumaz, tahmin olur.
   - IR v4: `Item::Constant`.

## Kapsam
**Var:** kararlar 1–6; `--prelude` (CLI ve builder); sentetik çeviri birimiyle header okuma; demo crate + C++ stub'ları + `ferrobridge.toml`; yeniden ölçüm.

**Yok:** template örnekleme, public alanlar, operatörler (M6); CMake (v0.2); demo için CI (Faz 7 / v0.1, CI kurulumuyla birlikte).

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| libclang erişimi | `crates/ferrobridge-parser/src/clang.rs:110` | `parse`; yeni çağrılar (unsaved file, evaluate) yalnızca burada ve `// SAFETY:` ile |
| Ana dosya filtresi | `crates/ferrobridge-parser/src/lower.rs:68` | `is_in_main_file` → "hedef header'da mı" |
| İç bağlantı kuralı | `crates/ferrobridge-parser/src/lower.rs:128` | `REASON_INTERNAL`; tanımı olan `static inline` için gevşer |
| Değişkenler | `crates/ferrobridge-parser/src/lower.rs:104` | `VarDecl` → bugün `unsupported`; sabitse `Item::Constant` |
| Açıklama biçimi | `crates/ferrobridge-analysis/src/config.rs` | `ParamAnnotation`: string ya da `{ slice = "..." }` tablosu (`serde(untagged)`) |
| Pointer kuralları | `crates/ferrobridge-analysis/src/rules.rs` (`param_type`) | `Pass::AddressOf` gibi yeni `Pass::Slice { len_index }` |
| Shim gövdesi | `crates/ferrobridge-codegen/src/shim.rs` (`body`) | Uzunluk kontrolü + `p.data()`, `static_cast<N>(p.size())` |
| Güvenli katman | `crates/ferrobridge-codegen/src/safe.rs` | `pub const`; slice'lar doğrudan geçer |
| `build.rs` | `crates/ferrobridge-e2e/build.rs` | Elle yazılmış `cxx_build` kodu → `ferrobridge_build` ile değiştirilir (dogfooding) |
| Tests | `crates/ferrobridge-parser/tests/fixtures.rs`, `crates/ferrobridge-e2e/tests/*.rs` | Snapshot + gerçek C++ ile e2e |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge-parser/src/{clang,lib,lower}.rs` | UPDATE | Sentetik çeviri birimi, `prelude`, hedef dosya filtresi, `static inline`, sabit değerlendirme |
| `crates/ferrobridge-ir/src/{item,lib}.rs` | UPDATE | `Item::Constant`, IR v4 |
| `crates/ferrobridge-analysis/src/{config,rules,lib,plan}.rs` | UPDATE | Slice açıklaması, `Pass::Slice`, `plan.constants` |
| `crates/ferrobridge-codegen/src/{bridge,shim,safe,spelling}.rs` | UPDATE | `&[T]` / `&mut [T]` bridge ve shim'leri, sabitler |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | `--prelude` (parse / generate / suggest) |
| `crates/ferrobridge-build/{Cargo.toml,src/lib.rs}` | UPDATE | Builder API, iki mod, `generate` özelliği |
| `crates/ferrobridge-e2e/build.rs` | UPDATE | `ferrobridge-build` kullanımı |
| `tests/fixtures/{self_include,inline_and_constants,slices}/` | CREATE | Bulgu 1, 3, sabitler, slice |
| `demo/ap_math/{Cargo.toml,LICENSE,README.md,build.rs,ferrobridge.toml,src/**,stubs/**,tests/**}` | CREATE | Demo crate (ayrı workspace) |
| `docs/adr/0008-*.md`, `docs/adr/README.md` | CREATE / UPDATE | Kararlar 2–3 |
| PRD (iki kopya), `README.md`, `CLAUDE.md`, Obsidian, `.claude/plans/m5-apmath-olcum.md` | UPDATE | Milestone bölünmesi, kullanım, yeniden ölçüm |

## Tasks
Her görev TDD ile yapılır: önce başarısız test, sonra kod, sonra doğrulama.

### Task 0: Riskli kısımlar için deneme (repo dışında)
- **Action**:
  - `AP_Math`'in `.cpp` dosyalarını `cc` ile derleyip küçük bir Rust programına bağla; hangi sembollerin (`AP_HAL`, `AP_InternalError`, `hal`...) stub istediğini çıkar. Ölçümdeki bulgu 4, M5'in en büyük riski.
  - cxx ile `static inline` fonksiyonu doğrudan bağla.
  - `rust::Slice<const uint8_t>` alan bir shim yaz.
- **Validate**: Deneme crate'i derleniyor ve çalışıyor; gereken stub listesi plan notlarına yazılır.

### Task 1: Sentetik çeviri birimi ve `prelude`
- **Action**:
  - Parser, header'ı doğrudan değil, bellekte üretilen `#include "<prelude>"… #include "<header>"` dosyası üzerinden okur (`CXUnsavedFile`).
  - Yalnızca hedef header'da bildirilen öğeler alınır.
  - `ParseOptions.prelude: Vec<String>`; CLI'da `--prelude` (tekrarlanabilir).
- **Mirror**: `clang.rs:110`, `lower.rs:68`
- **Validate**:
  - Yeni `self_include` fixture'ı (kendini dolaylı olarak include eden header) okunuyor.
  - Bütün mevcut parser snapshot'ları **değişmeden** geçiyor (konumlar ve öğeler aynı).

### Task 2: `static inline` ve sabitler (IR v4)
- **Action**:
  - Tanımı header'da olan iç bağlantılı fonksiyonlar `Item::Function` olur; tanımsız olanlar `unsupported` kalır.
  - `const`/`constexpr` ilkel değişkenler → `Item::Constant { name, namespace, ty, value }`. `value`: `Int(i128)` / `Float(f64)` / `Bool`; hesaplanamayan ya da sonlu olmayan değer `unsupported` olur.
  - `clang_Cursor_Evaluate` ve `clang_EvalResult_*` çağrıları `clang.rs`'e, `SAFETY` yorumlarıyla eklenir.
- **Mirror**: `lower.rs:104`, `:128`
- **Validate**: `inline_and_constants` fixture snapshot'ı (int, float, double, bool, `NaN`, `constexpr`, hesaplanamayan); IR round-trip testleri

### Task 3: Slice açıklaması (analiz)
- **Action**:
  - `ParamAnnotation::Slice { len }`.
  - Kurallar: pointer sayı tipini göstermeli; `len` aynı fonksiyonda tamsayı tipli bir parametre olmalı; aynı `len` iki slice'a bağlanamaz.
  - `Pass::Slice { len_index }`; uzunluk parametresi Rust imzasından çıkar.
  - Hatalar satır numaralı: olmayan `len`, tamsayı olmayan `len`, sınıf pointer'ı.
  - Varsayım metni; `suggest` taslağında pointer + tamsayı çifti için `slice` seçeneği yorum olarak.
- **Mirror**: `rules.rs` (`param_type`), `config.rs`
- **Validate**: `cargo test -p ferrobridge-analysis` (tablo testleri)

### Task 4: Kod üretimi
- **Action**:
  - Bridge'de `&[T]` / `&mut [T]`.
  - Shim: `::rust::Slice<const T>`. Uzunluk kontrolü: `if (s.size() > std::numeric_limits<N>::max()) throw …;`. Çağrı: `f(s.data(), static_cast<N>(s.size()))`.
  - Güvenli katman: slice'lar olduğu gibi geçer; sabitler `pub const` olur.
- **Mirror**: `shim.rs`, `safe.rs`
- **Validate**: codegen snapshot'ları + `slices` ve `inline_and_constants` için e2e testleri (boş slice, sınırı aşan uzunluk → `Err`, `&mut [f32]` yerinde değişiyor)

### Task 5: `ferrobridge-build`
- **Action**:
  - `Bridge` builder.
  - `compile()`: commit'li `bridge.rs` + `shim.cc` + kullanıcı kaynakları → `cxx_build`. `cargo:rerun-if-changed` satırları yazılır.
  - `generate` özelliği: `OUT_DIR`'e üretir, `#[path]` yerine `include!(concat!(env!("OUT_DIR"), …))` ile kullanılır. Üretilmiş dosya `include!` uyumlu olmalı; M4'te inner attribute kullanılmamıştı.
  - Açıklama hataları ve bağlanmayan öğeler `cargo:warning=` olarak basılır.
  - `ferrobridge-e2e/build.rs` bu crate'e geçirilir.
- **Mirror**: `crates/ferrobridge-e2e/build.rs`
- **Validate**: `cargo test -p ferrobridge-e2e` (commit'li mod); `generate` modu için `ferrobridge-build`'in kendi entegrasyon testi geçici bir crate'i derliyor

### Task 6: `AP_Math` demosu
- **Action**:
  - `demo/ap_math/` (ayrı workspace, GPL-3.0-or-later).
  - `build.rs`: `ARDUPILOT_DIR` yoksa ne yapılacağını söyleyen açık bir hata; varsa `ferrobridge-build` (`generate`) ile `AP_Math.h`, `control.h`, `crc.h`, `location.h`, `SCurve.h`, `SplineCurve.h`, `rotations.h`, `definitions.h`.
  - SITL define'ları, `hwdef.h` stub'ı, Task 0'da çıkan C++ stub'ları.
  - `ferrobridge.toml`: `control.h` çıkış parametreleri `borrowed`, `crc_*` ve `fill_nanf` `slice`.
  - Testler: CRC değerleri bilinen test vektörleriyle (ör. CRC-32 `"123456789"` → `0xCBF43926`), açı dönüşümleri, `SCurve` / `SplineCurve` iz hesabı, sabitler, `Rotation` enum'u.
  - README: ArduPilot'u seyrek klonlama komutları ve çalıştırma.
  - Yeniden ölçüm: ölçüm raporuna "M5 sonrası" bölümü.
- **Validate**: `ARDUPILOT_DIR=… cargo test --manifest-path demo/ap_math/Cargo.toml`; demo'da `unsafe` yok; kapsam hedefleri (≥ %85 template'siz, ≥ %30 toplam)

### Task 7: İnceleme, ADR ve belgeler
- **Action**:
  - `/ecc:security-review` (odak: slice uzunluk kontrolü, sabitlerin doğruluğu, `static inline` ile ODR, sentetik çeviri biriminde yol kaçışı) + `/ecc:cpp-review` (stub'lar).
  - ADR-0008.
  - PRD'de milestone bölünmesi (iki kopya), README (`build.rs` kullanımı, slice), CLAUDE.md, Obsidian.
- **Validate**: README'deki `build.rs` örneği e2e'nin `build.rs`'iyle aynı

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
ARDUPILOT_DIR=/yol/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| `AP_Math` `.cpp`'leri bağlanırken `AP_HAL` ve `AP_InternalError` gibi zincirleme bağımlılıklar çıkar (ölçümde `AP_Math.h` 22 ek kütüphane çekiyordu) | Yüksek | Task 0 önce. Stub'lar küçük kalmazsa demo yalnızca bağımlılığı az olan `.cpp`'lerle (`crc`, `control`, `location`, `SCurve`) başlar; kalanlar ölçümde "derlenemedi" olarak raporlanır |
| Sentetik çeviri birimi konumları ya da filtrelemeyi değiştirir, mevcut snapshot'lar kayar | Orta | Task 1'in ölçütü: mevcut snapshot'lar birebir aynı kalmalı |
| Sabit değerlendirme kayan noktada hassasiyet kaybeder (`float` sabit `double` olarak hesaplanır) | Orta | `float` sabitler `f32`'ye çevrilip geri `double`'a döndüğünde aynı değer veriyor mu kontrol edilir; vermiyorsa reddedilir. Snapshot'ta tam değerler |
| Slice uzunluğu `N`'ye sığmıyor (`uint16_t len`, 70 000 elemanlı slice) | Orta | Shim kontrolü → `Err`; e2e testi |
| `static inline` fonksiyonun adresi alınırken ODR veya görünürlük sorunu | Düşük | Task 0'da doğrulanır; sorun çıkarsa shim üzerinden çağrılır |
| GPL'den türemiş üretilmiş kod yanlışlıkla commit'lenir | Düşük | Demo `OUT_DIR`'e üretir; `demo/ap_math/.gitignore`; ADR-0008 |
| Kapsam (Large) | Orta | Template'ler, alanlar, operatörler ve CI bilinçli olarak dışarıda |

## Acceptance
- [x] `vectorN.h`, `matrixN.h`, `rotations.h` okunabiliyor; mevcut parser snapshot'ları değişmedi (Task 1'de birebir aynıydı; Task 2'de yalnızca IR sürümü 3 → 4 ve iki gerekçe metni değişti)
- [x] `static inline` fonksiyonlar ve sonlu sabitler bağlanıyor; `NaN` sabit açıklamalı olarak reddediliyor
- [x] `{ slice = "len" }` açıklamasıyla `&[T]` / `&mut [T]` çalışıyor; sınırı aşan uzunluk `Err`
- [x] `ferrobridge-e2e` kendi `build.rs`'inde `ferrobridge-build` kullanıyor
- [x] Demo gerçek bir ArduPilot checkout'una karşı derleniyor; testleri geçiyor, `unsafe` yok, repoda GPL'den türemiş dosya yok
- [x] Yeniden ölçüm: template olmayan API ≥ %85, toplam ≥ %30
- [x] ADR-0008 yazıldı; güvenlik ve C++ incelemesi yapıldı; PRD'de milestone'lar bölündü
- [x] Patterns mirrored, not reinvented

## Uygulama notları

### Task 0 (2026-11-27): riskler küçük çıktı
- **ArduPilot checkout'u:** `~/ardupilot`, commit `437a3301cc`. Ölçümdeki `32e4f8e`'den farklı bir commit; yeniden ölçüm bu commit'le yapılacak.
- **`AP_Math` derleniyor.** 18 `.cpp`'nin hepsi şu ayarlarla derleniyor:
  - `-std=c++17`
  - ölçümdeki SITL define'ları ve boş `hwdef.h`
  - waf'ın eklediği `__AP_LINE__=__LINE__`, `__STDC_FORMAT_MACROS=1`
  - `SCurve.cpp` için `APM_BUILD_DIRECTORY=APM_BUILD_UNKNOWN`
  - `matrix_alg.cpp` için `-I libraries/AP_Common/missing`. macOS'ta `feenableexcept` buradan geliyor.
- **Bağlamak için gereken stub'lar (bulgu 4) yalnızca dört parça; tek bir `.cc` yetiyor:**
  - `AP_InternalError::error` + `AP::internalerror()`. Hata kaydedilir, program durmaz.
  - `AP_CustomRotations::{from_rotation, rotate}` ×2 + `AP::custom_rotations()`. Yalnızca `ROTATION_CUSTOM_*` için; çağrılırsa internal error kaydedilir.
  - `AP_HAL::panic`: mesajı basar, sonra `abort`.

  Gerisi libc/libm. Planın B planına (yalnızca az bağımlı `.cpp`'ler) gerek kalmadı.
- **`static inline constexpr` fonksiyonlar cxx ile doğrudan bağlanıyor**, aşırı yüklenmiş `radians(float)` de (`#[cxx_name]`). cxx fonksiyon pointer'ını imzaya çevirdiği için overload doğru seçiliyor. ODR ya da görünürlük sorunu çıkmadı.
- **`rust::Slice<const uint8_t>` shim'i çalışıyor.** `crc_crc32(0xFFFFFFFF, "123456789")` sonucu tersine çevrilince `0xCBF43926` veriyor. Boş slice çalışıyor. `crc_crc8`'e 300 baytlık slice verilince `uint8_t` uzunluk kontrolü `Err` döndürüyor.
- **M6 için ipucu:** `wrap_180` bir fonksiyon template'i, ama `AP_Math.cpp`'de `int`/`short`/`float`/`double` için açıkça örnekleniyor. cxx `float(*)(float)` dönüşümüyle doğru örneği seçiyor. Fonksiyon template'leri açık örneklemeleri üzerinden bağlanabilir.

### Task 1 (2026-11-27): sentetik çeviri birimi
- **Sentetik birim:** Header'ın yanında, bellekte `<header>.ferrobridge.cc` üretiliyor (`CXUnsavedFile`). İçinde `#include "<mutlak yol>"` var.
- **Hedef dosya filtresi:** `clang_getFile` + `clang_File_isEqual` kullanılıyor. Karşılaştırmada makro açılım konumuna bakılıyor; eski `clang_Location_isFromMainFile` de öyleydi. Mevcut 9 snapshot birebir aynı kaldı.
- **Sapma 1: `prelude` clang'ın `-include` argümanına çevriliyor.** Planda prelude sentetik birime `#include` satırı olarak yazılacaktı. `-include`'un çözümleme kuralı clang'da zaten tanımlı (önce çalışma dizini, sonra include yolları); kendi kuralımızı uydurmuyoruz. Sentetik birimde yalnızca header'ın satırı kalıyor.
- **Sapma 2: prelude için ayrı bir `prelude` fixture'ı açıldı.** `self_include` fixture'ına eklemek iki ayrı sorunu karıştırıyordu.
- **Yeni sorun: dosya sonu hataları yanlış konum gösteriyordu.** Header'da kapanmayan bir parantez dosya sonunda fark ediliyor; dosya sonu da artık sentetik birimde. Bu yüzden tanı `input.h.ferrobridge.cc:1:85` gösteriyordu.
  - Çözüm: böyle bir tanı, başka dosyayı gösteren ilk notuna ("to match this '('") taşınıyor ve notun metni mesaja ekleniyor.
  - Not yoksa tanı olduğu gibi kalıyor; hiçbir tanı atılmıyor.
- **Yol kısıtı:** `"` ya da satır sonu içeren header yolları `#include` satırına yazılamaz; `InvalidArgument` olarak reddediliyor.
- **CLI:** `std`, `prelude` ve clang argümanları tek bir `ParseArgs` yapısında (`#[command(flatten)]`). `--prelude` `parse`, `generate` ve `suggest`'te tekrarlanabilir.
- **ArduPilot'ta doğrulama:**
  - `vectorN.h` ve `matrixN.h` ek argüman olmadan okunuyor (bulgu 1).
  - `rotations.h`, `SCurve.h` ve `SplineCurve.h`, `--prelude AP_Math/AP_Math.h` ile okunuyor (bulgu 2).

### Task 2 (2026-11-27): `static inline` ve sabitler (IR v4)
- **Sapma 1: fonksiyon gövdeleri artık atlanmıyor (`SkipFunctionBodies` kaldırıldı).**
  - libclang gövdesi atlanan fonksiyonu "tanım" saymıyor. `clang_isCursorDefinition` ve `clang_getCursorDefinition` ikisi de gövdeye bakıyor. Bu yüzden `static inline int f() { ... }` tanımsız görünüyordu.
  - Ölçüm: `AP_Math.h` ve çektiği 28 kütüphane, gövdeler atlanınca 0.54 s, atlanmayınca 0.45 s. Fark yok.
  - Header'daki gövdeler shim derlenirken zaten derleniyor; hataları artık `generate` sırasında görünüyor.
- **Anonim namespace'teki fonksiyonların bağlantısı C++11'den beri `Internal`, `UniqueExternal` değil.** Bu yüzden namespace yolundan tanınıyorlar ve ayrı bir gerekçe alıyorlar ("Rust'tan yazılabilecek adı yok").
- **Sapma 2: `ConstantValue` tamsayılar için `Signed(i64)` / `Unsigned(u64)` olarak ikiye bölündü.** Planda `Int(i128)` vardı, ama `serde_json`'un `Value` tipi (ve insta snapshot'ları) `i128` taşıyamıyor. `Eq` elle yazıldı: değerler her zaman sonlu.
- **Ek güvenlik kuralı: başlatıcısında fonksiyon çağrısı olan sabit reddediliyor.** Sabit başka bir sabite dayanıyorsa zincir izleniyor.
  - Neden: `static const double X = sqrt(2.0)` bir C++ sabit ifadesi değil. Derleyici değeri çalışma anında libm ile hesaplayabilir; libclang ise derleme anında kendisi katlıyor. İki sonuç son bitte farklı çıkabilir.
  - ArduPilot'ta bu kural nedeniyle `DEG_TO_RAD_DOUBLE` (`asin`), `RAD_TO_DEG_DOUBLE` (ona dayanıyor), `WGS84_E` (`sqrt`) ve `NaNf` (`nanf`) bağlanmıyor. `WGS84_A/IF/F/B` bağlanıyor.
- **Analizde geçici kayıt:** Analiz `Item::Constant`'ı şimdilik "henüz bağlanmıyor" diye atlanan öğe olarak kaydediyor. Task 4'te gerçek bağlamayla değişecek.
- **Snapshot'lar:** 12 snapshot `format_version: 4` oldu. `namespaces` snapshot'ında iki gerekçe metni değişti. 144 test geçiyor.

### Task 3 (2026-11-27): slice açıklaması (analiz)
- **Plan biçimi:**
  - Pointer parametresi `BridgeType::Slice { mutable, element }` ve `Pass::Slice` oluyor.
  - Uzunluk parametresi `FunctionPlan.params`'ta yer almıyor; `FunctionPlan.lengths: Vec<LengthParam { position, slice, cpp_type }>` olarak tutuluyor. `position` C++'taki sırası, `slice` Rust'taki slice parametresinin yeri, `cpp_type` C++ temel tipi (`"unsigned short"`).
  - Bu sayede `size_t` gibi Rust'a taşınamayan tipler de uzunluk olabiliyor. Planda yazan `Pass::Slice { len_index }` yerine bu yapı seçildi, çünkü shim'in C++ sırasını ve uzunluğun tipini bilmesi gerekiyor.
- **`ParamAnnotation` artık `Copy` değil (`Slice { len: String }`).** Deserialize elle yazıldı. `serde(untagged)` "data did not match any variant" gibi ipucu vermeyen bir mesaj üretiyordu.
- **Yanlış slice açıklamaları satır numaralı hata, uyarı değil.** Hata sayılan durumlar:
  - olmayan `len`
  - tamsayı olmayan `len`
  - `len`'in parametrenin kendisi olması
  - sayı olmayan eleman (sınıf, `char`, `bool`)
  - pointer olmayan parametre
  - iki slice'ın aynı `len`'i paylaşması
  - `len`'e ayrıca açıklama verilmesi
- **Eleman tipi:** sabit genişlikli tamsayı, `float` ya da `double`. `char` reddediliyor (işareti platforma bağlı); mesaj `uint8_t` öneriyor.
- **`suggest`:** Sayı pointer'ı için `borrowed` satırının altına, aynı fonksiyondaki her tamsayı parametresi için `# p = { slice = "n" }` seçeneği yazılıyor.
- **Davranış değişikliği:** Bir tamsayı parametresinin reddi (ör. `size_t`), aynı fonksiyonda slice olabilecek bir pointer varsa "açıklamayla çözülebilir" sayılıyor. Böylece `crc(const uint8_t*, size_t)` için öneri basılıyor.
  - M3'ün `no_suggestions_when_an_annotation_would_not_help` testi `int* p, long n` örneğini kullanıyordu. Bu imza artık bağlanabildiği için örnek "bilinmeyen sınıf pointer'ı + `long`" ile değiştirildi.
- **Test yardımcısı `uncomment_first_choices`:** Artık her tablo ve anahtar için yalnızca ilk seçeneği açıyor ve taslağın açıklama başlığındaki örnek satırlara dokunmuyor.

### Task 4 (2026-11-27): slice ve sabitler için kod üretimi
- **Shim:**
  - Argümanlar C++ sırasıyla kuruluyor; uzunluğun yerine `static_cast<N>(s.size())` yazılıyor.
  - Uzunluk kontrolü `unsigned long long` cinsinden yapılıyor; iki taraf da negatif değil ve hiçbir değer kesilmiyor. Sığmazsa `std::length_error` fırlatılıyor.
  - `shim.h`, yalnızca slice varsa `<limits>` ve `rust/cxx.h` include ediyor. Mevcut 5 fixture'ın çıktısı değişmedi.
- **Ek kural: uzunluk parametresi olan her fonksiyon `Result` döndürüyor, `noexcept` olsa da.** Kontrol `noexcept` bir shim'de `std::terminate` olurdu. e2e'de `count_nonzero(... ) noexcept`'e 65 536 elemanlık slice verilince `Err` dönüyor.
- **Sabitlerin analizi:**
  - Analiz `plan.constants` üretiyor ve tip tablosu uygulanıyor. Platforma bağlı `long`/`size_t` sabitleri reddediliyor.
  - Değerin tipe sığması kontrol ediliyor. `char` sabitleri yalnızca 0–127 arasında kabul ediliyor.
  - Aynı ad farklı namespace'lerde varsa ön ek alıyor (`metric_UNIT`).
  - Bir güvenli fonksiyonla aynı adı taşıyan sabit atlanıyor (Rust'ta ikisi aynı ad alanında).
- **Sabitlerin üretimi:**
  - Güvenli katmanda `pub const`; `float` sabitler `f32`'nin en kısa yazımıyla (`9.80665`), negatifler `-` + mutlak değer.
  - `#[allow(non_upper_case_globals, clippy::approx_constant)]`.
- **e2e kanıtı:** `WGS84_F` (`1.0 / WGS84_IF`) ve `TENTH` (`0.1` → `float`) için Rust `const`'ı, C++'ın çalışma anındaki değeriyle `to_bits()` düzeyinde aynı.
- **Fixture değişiklikleri:**
  - `inline_and_constants`: `int counter` → `inline int counter`. Header birden fazla çeviri birimine girdiği için aksi halde bağlama hatası çıkıyordu.
  - `triple` ve `hidden` `[[maybe_unused]]` aldı; C++ uyarıları kaynağında susturuldu.
  - Sona `wgs84_f_at_runtime` / `tenth_at_runtime` eklendi.
- **CLI testindeki `uncomment_first_choices` kopyası da analizdeki gibi düzeltildi.**
- **Toplam:** 164 test (Task 3–4'te +20), clippy ve `cargo doc -D warnings` temiz.

### Task 5 (2026-11-27): `ferrobridge-build`
- **Sapma: API, tek header'lık zincir (`Bridge::new(header)...compile(name)`) yerine ortak bir `Build`.**
  - Hem e2e (7 fixture) hem demo (8 header) birden fazla bridge'i aynı include yolları, define'lar, C++ standardı ve kaynaklarla tek kütüphanede derliyor.
  - `Build::new().committed(dir).generate(Generate::new(header, "ad").include_as(..).config(..).prelude(..)).include_dir(..).define(..).file(..).std(..).flag(..).compile(lib)`.
  - `generate` modunda include dizinleri, define'lar ve standart libclang'a da aynen geçiyor; `flag` yalnızca C++ derleyicisine.
- **Rust tarafı:** `ferrobridge::include_bridge!("ad")` makrosu (çalışma zamanı crate'inde) `$OUT_DIR/ferrobridge/<ad>/bridge.rs`'i ekliyor. `cxx-build` `OUT_DIR` içindeki bridge dosyalarını açıkça destekliyor.
- **Prelude artık bir codegen ayarı (`CodegenOptions.prelude`).**
  - Neden: header kendi başına derlenmiyorsa shim de prelude'u header'dan önce include etmeli. Bu, CLI'daki `generate --prelude` için de geçerli. İlk denemede bu ekleme `ferrobridge-build`'de metin değiştirerek yapılıyordu.
- **Uyarılar ve hatalar:**
  - Açıklama hataları ve libclang tanıları derlemeyi durduruyor; mesaj satır satır.
  - Atlanan öğeler için bridge başına tek bir özet `cargo:warning` basılıyor; liste üretilen dosyanın başında. Plandaki "her öğe için bir uyarı" `AP_Math`'te yüzlerce satır olurdu.
- **Dosyalar ve yeniden derleme:**
  - Üretilen dosyalar yalnızca içerik değiştiyse yazılıyor; gereksiz C++ derlemesi olmuyor.
  - `rerun-if-changed` header, config, kaynaklar ve include dizinleri için basılıyor.
- **Dogfooding:** `ferrobridge-e2e/build.rs` artık `ferrobridge-build`'in commit'li dosya modunu kullanıyor; 41 e2e testi geçiyor.
- **`generate` modu entegrasyon testi (`tests/generate_mode.rs`):**
  - Geçici bir crate'in `build.rs`'i header'ı okuyup derliyor. Kapsananlar: slice, `static inline`, `constexpr` sabit, prelude, `typedef`'li dönüş.
  - Hatalı bir açıklama derlemeyi `ferrobridge.toml:2` ile durduruyor.
  - Ağ kullanılmıyor: `--offline`, workspace `Cargo.lock`'u kopyalanıyor. Hedef dizini ortak: `target/ferrobridge-build-it/`.
- **Toplam:** 169 test. Clippy iki modda da temiz (özellikli ve özelliksiz). `cargo doc -D warnings` temiz.

### Task 6 (2026-11-27): `AP_Math` demosu
- **Yapı:** `demo/ap_math/` ayrı bir workspace (`[workspace]`), `GPL-3.0-or-later`, `publish = false`.
  - `LICENSE`, ArduPilot checkout'undaki GPLv3 metninin kopyası. Bu lisans metni, kod değil; GPL de kopyasının konmasını istiyor.
  - Repoya giren 17 dosyanın hiçbiri üretilmiş değil; bridge'ler `target/…/out/ferrobridge/` altında.
- **`build.rs`:** 8 header × `Generate`, prelude `AP_Math/AP_Math.h`. Bunlara ek olarak `AP_Math`'in 18 `.cpp`'si, `stubs/ap_stubs.cc`, Task 0'daki define'lar ve `-w` (ArduPilot'un kendi uyarıları; libclang'a geçmiyor). `ARDUPILOT_DIR` yoksa açık bir hata veriyor.
- **Sapma 1: açıklamalar tek bir `ferrobridge.toml` yerine header başına bir dosyada (`annotations/<bridge>.toml`).** Analiz, açıklanan her adın o header'da bulunmasını şart koşuyor (M3: yazım hatalarını yakalamak için). Ortak bir dosya her bridge'de "header'da bulunamadı" hatası verirdi. İleride `ferrobridge.toml`'da header bölümleri düşünülebilir.
- **Sapma 2: crate düzeyinde `forbid(unsafe_code)` yerine `deny(unsafe_code)`; bridge modüllerinde gerekçeli `#[allow(unsafe_code)]`.** `include_bridge!` cxx katmanını kullanıcının crate'ine ekliyor ve cxx'in açılımı `unsafe extern` içeriyor; `forbid` gevşetilemiyor. Testler `forbid(unsafe_code)` ile derleniyor. `include_bridge!` belgesine not düşüldü.
- **Açıklamalar kaynak okunarak yazıldı.** `crc_crc4` (uzunluksuz, sabit 8 kelime) bilinçli olarak açıklanmadı. `crc_crc16_ibm`'in `const` olmayan ama yalnızca okunan pointer'ı gereğinden kısıtlayıcı `&mut [u8]` oluyor.
- **Testler:** 17 test, gerçek ArduPilot'a karşı. CRC'ler standart katalog değerleriyle doğrulandı: CRC-32 `0xCBF43926`, XMODEM `0x31C3`, MODBUS `0x4B37`, IBM-3740 `0x29B1`, MAXIM `0xA1`, SMBUS `0xF4`, DVB-S2 `0xBC`. Ayrıca FNV-1a referansı, 256 baytlık slice'ın `Err` dönmesi, açılar, `control.h` çıkış parametreleri, `SCurve` (sahip olunan nesne, statik metot), WGS84 sabitlerinin bit bit doğruluğu ve `Rotation` enum'u.
  - `crc8_dvb_update` için ilk beklentim (`0xBC`) yanlıştı: fonksiyon 0x07 polinomunu kullanıyor (CRC-8/SMBUS, `0xF4`). Test kaynağa dayanarak düzeltildi; `crc8_dvb_s2` ile DVB-S2 ayrıca doğrulandı.
- **Yeniden ölçüm:** template olmayan API 131/146 = **%90** (hedef ≥ %85), toplam 131/424 = **%31** (hedef ≥ %30). Açıklamasız: %68 / %23. Ayrıntı: `m5-apmath-olcum.md` → "M5 sonrası".
- **CLI özet satırı artık sabitleri de sayıyor.**
- **Doğrulanmayanlar:** Seyrek klonla derleme (ağ gerektiriyor; README'de belirtildi). Linux.

### Task 7 (2026-11-27): inceleme, ADR-0008, belgeler
- **Güvenlik incelemesi (`/ecc:security-review`, odaklı).** Üç bulgu düzeltildi:
  1. **Sabit başlatıcı denetimi üstel sürüyordu.** `A2 = A1 + A1` zincirinde, 60 adımda üretici donuyordu; test 60 saniyede zaman aşımına uğradı. Düzeltme: `InitializerCheck` doğrulanan değişkenleri hatırlıyor ve toplam düğüm sınırı var (100 000; aşılırsa ihtiyatla reddediliyor). Test: `chained_constants_are_checked_in_linear_time`.
  2. **Platforma bağlı sabitler.** `sizeof`/`alignof` (`CXCursor_UnaryExpr`) ve `offsetof` (alana nesnesiz başvuru, `CXCursor_MemberRef`) içeren başlatıcılar reddediliyor. Ayrıca tamsayı/`bool` sabitler `shim.cc`'de `static_assert(::ns::X == değer)` ile C++ derlemesinde yeniden doğrulanıyor; commit'li dosya başka platformda ya da başka define'larla derlenince fark derleme hatası oluyor. Kayan noktalı sabitler `static_assert`'te kullanılamıyor; onları başlatıcı denetimi koruyor.
  3. **Include yollarıyla üretilen C++'a satır enjeksiyonu.** `--include-as`, `--shim-include` ve `prelude` içinde `"`, satır sonu ya da NUL varsa `CodegenError::InvalidInclude` dönüyor.
- **Sorunsuz bulunanlar:**
  - `CXUnsavedFile`: libclang içeriği kopyalıyor.
  - Alt tanıların kümesi serbest bırakılmıyor (belgeye uygun).
  - Sentetik birimde yol kaçışı yok.
  - `include_bridge!` / `OUT_DIR` yazımı: ad tanımlayıcı olarak doğrulanıyor.
  - Argüman sırası ve uzunluk kontrolü doğru.
- **C++ incelemesi (`/ecc:cpp-review`, `ecc:cpp-reviewer` ajanı): kritik bulgu yok.**
  - Negatif `char` sabitlerinin `static_assert` uyumsuzluğu ve `__int128` uzunluk tipi analizde zaten engellenmiş durumda (`char` 0–127; 128 bit tamsayı değil).
  - Belgelenenler:
    - boş slice'ın null olmayan pointer'ı (README ve verilen söz metni)
    - `static` fonksiyonlardaki yerel durumun ayrı kopya olması (README)
    - stub sınırları: atomik değil, özel döndürmede `abort`, gerçek ArduPilot'la sembol çakışması (stub dosyasının başı)
  - **Açık kalan:** Uzunluk tipi üretim platformundaki temel ad olarak yazılıyor (`size_t` → `unsigned long`). LLP64'te (Windows) 4 GiB'den uzun slice sahte `Err` alır. Güvenli yönde; Windows desteğiyle birlikte ele alınmalı.
- **Test düzeneği hatası düzeltildi:** `generate_mode.rs`'teki iki geçici crate aynı paket adını taşıyordu. Cargo kök paketin kimliğini yoldan bağımsız hesapladığı için hatalı crate öbürünün `build.rs` çıktısını "güncel" sayıyordu. Her crate'e ayrı ad verildi; art arda iki çalıştırmada da geçiyor.
- **ADR-0008 yazıldı;** ADR-0004'e ona işaret eden bir not eklendi.
