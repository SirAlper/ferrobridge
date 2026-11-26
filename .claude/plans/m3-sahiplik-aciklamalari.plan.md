# Plan: Milestone 3 — Sahiplik açıklamaları

**Source PRD**: `.claude/prds/ferrobridge.prd.md`
**Selected Milestone**: 3 — Sahiplik açıklamaları ("Kullanıcı belirsiz pointer'lar için açıklama yazar; araç belirsiz durumda kod üretmek yerine soru sorar")
**Complexity**: Large
**Durum**: Tamamlandı (2026-11-27, 4 kararla birlikte onaylandı)

## Summary
C++ header'ı bir pointer'ın sahibini ve ömrünü söylemez; ferrobridge bunu tahmin etmez (güvenlik kuralı 1). Bu milestone'da kullanıcı bu bilgiyi bir **`ferrobridge.toml`** dosyasında verir, ferrobridge de buna göre güvenli kod üretir: sahipliği devreden pointer `UniquePtr<T>` olur, ödünç verilen pointer `&T` / `Pin<&mut T>` olur, primitif çıkış parametresi (`double& out`) `&mut f64` olur. Açıklanmamış her pointer için kod üretilmez; bunun yerine yeni **`ferrobridge suggest`** komutu doldurulacak bir `ferrobridge.toml` taslağı basar ("soru sorar"). M2'den kayan **`std::vector<T>`** desteği de burada gelir.

## Güven modeli (en önemli fikir)
`ferrobridge.toml`'daki her açıklama, kullanıcının C++ kütüphanesi hakkında verdiği bir **sözdür** ("bu fonksiyon pointer'ı saklamıyor", "dönen nesneyi `new` ile ayırdı, silmek bana düşüyor"). Söz yanlışsa tanımsız davranış olabilir; Rust'taki `unsafe` bloğuna benzer. Bu yüzden:
- Söz yalnızca bu dosyada verilir; üretilen kod hiçbir şeyi kendiliğinden varsaymaz.
- Üretilen her dosyanın başında **"Açıklamaya dayanan varsayımlar"** listesi bulunur; kod incelemesinde güven noktaları tek bakışta görülür.
- Ucuz olan her kontrol yine de yapılır: ödünç alınan bir dönüş pointer'ı null gelirse shim onu referansa çevirmez, `Err` döndürür (UB yerine hata).

## Kapsam
**Var:**
- `ferrobridge.toml` biçimi ve doğrulaması: bilinmeyen fonksiyon/parametre adı, geçersiz değer, gereksiz açıklama → açık hata/uyarı (yazım hatası koruması).
- **Parametre** açıklaması `"borrowed"`: pointer/referans yalnızca çağrı süresince kullanılır, saklanmaz, null değildir.
  - `const T*` (T: sınıf, primitif, `std::string`) → `&T` (shim adresini alıp geçirir)
  - `T*` sınıf → `Pin<&mut T>`; `T*` primitif → `&mut T`
  - primitif `T&` (ör. `double& out`) → `&mut f64` (shim gerekmez)
- **Dönüş** açıklamaları:
  - `"owned"`: `T*` dönüşü, nesne `new` ile ayrılmış ve sahipliği çağırana geçiyor → `UniquePtr<T>` (null gelirse boş `UniquePtr`, UB yok)
  - `"borrowed"` (yalnızca metotlar): `T*` dönüşü nesnenin içini gösteriyor, nesne yaşadıkça geçerli → `Result<&T>` / `Result<Pin<&mut T>>`; null → `Err`
- `std::vector<T>` (T: primitif, `std::string`, bu header'daki sınıflar): `const std::vector<T>&` → `&CxxVector<T>`, `std::vector<T>&` → `Pin<&mut CxxVector<T>>`, değerle dönüş → shim ile `UniquePtr<CxxVector<T>>`. Açıklama gerekmez (değer semantiği).
- `ferrobridge generate --config ferrobridge.toml`
- `ferrobridge suggest <header> [--config ...]`: henüz açıklanmamış her pointer/referans için seçenekleri ve anlamlarını açıklayan, yorum satırlı bir `ferrobridge.toml` taslağı basar.

**Yok (gerekçeli):**
- Null olabilen parametreler (`Option<&T>`): cxx'te `Option` yok; M4'teki sarmalayıcı katmanda iki shim'le çözülebilir.
- Pointer + uzunluk çiftlerinden slice (`const T* data, size_t n` → `&[T]`): `size_t` de shim dönüştürmesi istiyor; ayrı iş, v0.2.
- `const char*` C string'leri, `void*`, fonksiyon pointer'ları, diziler → açıklamalı uyarıyla reddedilmeye devam eder.
- Serbest fonksiyonların döndürdüğü referanslar (`'static` ömür): cxx desteği doğrulanmadı; sonraki milestone.
- Header içinde C++ attribute'uyla açıklama (`[[clang::annotate]]`) → v0.2 (karar 2).
- Overload'ları ayrı ayrı açıklamak: M3'te anahtar tam C++ adıdır ve **aynı adlı tüm overload'lara** uygulanır; bir overload'da olmayan parametre adı hata verir.

## Karar gerektiren noktalar (onayınla kabul edilir)
1. **Güven modeli** (yukarıda): açıklama = kullanıcının sözü; üretilen dosyalarda varsayımlar listesi; ucuz kontroller (null) her zaman yapılır.
2. **ADR-0006: Açıklamalar ayrı bir TOML dosyasında.** Üçüncü taraf header'lar düzenlenemediği için ayrı dosya şart; C++ attribute desteği v0.2'ye. Bu, PRD'deki "açıklamaların yeri" açık sorusunu kapatır.
3. **Primitif `T&` için de açıklama şart kalır.** `03 Mimari` tip tablosundaki kararla tutarlı: `double& out` çok yaygın ama C++ fonksiyonu referansı saklayabilir; `"borrowed"` sözü olmadan `&mut` üretilmez. `suggest` bu durumları tek komutla listeler.
4. **Destructor kontrolü (2026-11-27 eklendi).** `UniquePtr<T>` düşünce `delete` çalışır. Bunun güvenli olması için iki koşul gerekir: (a) destructor public olmalı ve silinmemiş olmalı; (b) sınıf polimorfikse (sanal metodu varsa) destructor sanal olmalı. (b)'nin nedeni şu: "arayüz + fabrika" deseninde `Shape* make_shape()` aslında `.cc` içinde tanımlı bir alt sınıf döndürür. Destructor sanal değilse `delete` tanımsız davranıştır. Bugünkü IR destructor'ı hiç kaydetmiyor (`lower.rs:291` atlıyor). Bu yüzden M2'de de gizli bir hata var: private destructor'lı bir sınıfın constructor'ı `UniquePtr` döndürür ve üretilen kod derlenmez. Karar: IR v3 `Class.destructor` alanını kaydeder. `"owned"` dönüş, constructor fabrikası ve değerle dönüş (`BoxedClass`) bu iki koşulu kontrol eder; koşul tutmazsa öğe açıklamalı bir uyarıyla reddedilir. Bu, güven modelindeki "ucuz kontroller her zaman yapılır" ilkesinin bir uygulaması.

## Açıklama dosyası örneği
```toml
# ferrobridge.toml

# Dönen pointer `new` ile ayrıldı; silmek çağırana düşüyor.
[functions."geo::make_widget"]
returns = "owned"

# Dönen pointer nesnenin içini gösteriyor; nesne yaşadıkça geçerli.
[functions."geo::Widget::buffer"]
returns = "borrowed"

# Parametreler yalnızca çağrı süresince kullanılıyor, saklanmıyor, null değil.
[functions."geo::Widget::attach".params]
target = "borrowed"

[functions."geo::Vehicle::read_odometer".params]
out = "borrowed"
```

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Reddetme gerekçeleri | `crates/ferrobridge-analysis/src/rules.rs:11` | `REASON_*` sabitleri; "Milestone 3" ifadeleri artık açıklama dosyasına yönlendirecek |
| Tip kuralları | `crates/ferrobridge-analysis/src/rules.rs:50`, `:145` | `param_type` / `ref_type` kalıbı genişletilir; reddederken neden metni |
| Bağlama | `crates/ferrobridge-analysis/src/lib.rs:268` | `bind()`: tek parametre bile desteklenmiyorsa tümüyle atla |
| Plan tipleri | `crates/ferrobridge-analysis/src/plan.rs:168`, `:231` | `Returns` enum'u ve `needs_shim()` yeni durumlarla genişler |
| Shim üretimi | `crates/ferrobridge-codegen/src/shim.rs:86` | `body()`: çağrı ifadesi + dönüş sarmalaması |
| Rust tipleri | `crates/ferrobridge-codegen/src/spelling.rs:16` | `rust_type` / `cpp_type` birebir eşleşir |
| std tipleri | `crates/ferrobridge-parser/src/lower.rs:401` | `lower_record`: namespace + template argümanıyla tanıma (`unique_ptr` örneği `:416`) |
| CLI | `crates/ferrobridge-cli/src/main.rs:36` | `Generate` varyantı; yeni `Suggest` varyantı aynı desende |
| Tests | `crates/ferrobridge-analysis/src/tests.rs:124`, `crates/ferrobridge-cli/tests/generate.rs:36` | Tablo testleri; e2e senkron testi yeni fixture'ları da kapsar |
| Logging | — | Yok; uyarılar stderr'e `⚠` / `ℹ` ile |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/ferrobridge-ir/src/types.rs`, `item.rs`, `lib.rs` | UPDATE | `Type::Vector { element }`; `Class.destructor` (`Public { is_virtual }` / `NotPublic` / `Deleted`, bildirilmemişse örtük public); `IR_FORMAT_VERSION` → 3 |
| `crates/ferrobridge-parser/src/lower.rs` | UPDATE | `std::vector<T, std::allocator<T>>` tanıma; destructor erişimi, `= delete` ve sanallık (erişim filtresinden **önce** okunur) |
| `crates/ferrobridge-analysis/Cargo.toml` | UPDATE | `serde`, `toml` |
| `crates/ferrobridge-analysis/src/config.rs` | CREATE | `ferrobridge.toml` modeli, okuma, doğrulama, hata tipi |
| `crates/ferrobridge-analysis/src/rules.rs` | UPDATE | Açıklamalı pointer/referans kuralları, `std::vector` |
| `crates/ferrobridge-analysis/src/plan.rs` | UPDATE | `ParamPlan.pass` (`Direct` / `AddressOf`), `Returns::OwnedPointer`, `Returns::BorrowedPointer`, `BridgeType::Vector`, `BridgePlan.assumptions`, `BridgePlan.suggestions` |
| `crates/ferrobridge-analysis/src/lib.rs` | UPDATE | `check(&Module, &Annotations)`; kullanılmayan açıklamalar → hata/uyarı |
| `crates/ferrobridge-analysis/src/suggest.rs` | CREATE | Açıklanmamış yerler için TOML taslağı |
| `crates/ferrobridge-codegen/src/{spelling,shim,lib}.rs` | UPDATE | `&mut T`, `CxxVector<T>`, adres geçirme, sahip olan/ödünç pointer dönüşleri (null → exception → `Err`), varsayımlar bloğu |
| `crates/ferrobridge-cli/src/main.rs` | UPDATE | `generate --config`, `suggest` komutu |
| `tests/fixtures/ownership/{input.h,impl.cc,ferrobridge.toml}` | CREATE | Sahip olan/ödünç pointer'lar, çıkış parametreleri, **canlı nesne sayacı** |
| `tests/fixtures/vectors/{input.h,impl.cc}` | CREATE | `std::vector<int>`, `<double>`, `<std::string>`, `<Sınıf>` |
| `tests/fixtures/bad_config/` | CREATE | Bilinmeyen ad, bilinmeyen parametre, geçersiz değer, gereksiz açıklama |
| `crates/ferrobridge-e2e/{build.rs,src/lib.rs,tests/*.rs,README.md}` | UPDATE / CREATE | Yeni iki fixture |
| `crates/*/tests/snapshots/` | UPDATE | IR v3 ve yeni fixture'lar |
| `docs/adr/0006-*.md`, `docs/adr/README.md` | CREATE / UPDATE | Karar 2 |
| `README.md`, PRD, `CLAUDE.md`, Obsidian notları | UPDATE | Kullanım, M3 durumu, açık sorunun kapanması |

## Tasks
Her görev TDD ile: önce başarısız test, sonra kod, sonra doğrulama. Önce kısa bir **cxx denemesi** (M2'deki gibi, depo dışında): `&mut f64` parametresi, `CxxVector<i32>` / `CxxVector<CxxString>` / `CxxVector<OpakSınıf>`, `UniquePtr<CxxVector<T>>` dönüşü.

### Task 1: IR v3: `std::vector` ve destructor bilgisi
- **Action**: `Type::Vector { element: Box<QualType> }`; parser varsayılan allocator'lı `std::vector`'ü tanır, özel allocator'lı olanı reddeder. `Class.destructor`: private/protected/`= delete`/sanal destructor ayrımı. Bir taban sınıftan miras alınan sanallık M3'te önemli değil, çünkü kalıtım hâlâ reddediliyor.
- **Mirror**: `lower.rs:416` (`unique_ptr` tanıma), `clang.rs:354` (`is_virtual_method`)
- **Validate**: `cargo test -p ferrobridge-ir -p ferrobridge-parser` + yeni `vectors` ve `destructors` snapshot'larının incelenmesi

### Task 2: Açıklama dosyası (`config.rs`)
- **Action**: `Annotations::from_toml_str` / `from_file`; `functions.<ad>.returns` ∈ {`owned`, `borrowed`}, `functions.<ad>.params.<parametre>` ∈ {`borrowed`}. Bilinmeyen anahtar, geçersiz değer → satır numaralı hata.
- **Mirror**: `parser/src/lib.rs:66` (thiserror deseni)
- **Validate**: `cargo test -p ferrobridge-analysis config::`

### Task 3: Açıklamalı kurallar
- **Action**: `check(&Module, &Annotations)`. Pointer ve primitif `T&` artık açıklamaya bakar; açıklama yoksa reddeder ve bir **öneri** kaydeder. `"borrowed"` pointer parametreleri `pass = AddressOf` (shim gerekir). `"owned"` dönüş → `OwnedPointer`; metottan `"borrowed"` dönüş → `BorrowedPointer` (her zaman `fallible`). Kullanılan her açıklama `assumptions`'a yazılır. Hiçbir öğeye uymayan açıklama → hata; açıklama gerektirmeyen bir yere verilen açıklama → uyarı. `std::vector` kuralları. Destructor kontrolü (karar 4): `OwnedPointer`, `BoxedClass` ve constructor fabrikaları public destructor ister; polimorfik sınıflarda sanal destructor şart.
- **Mirror**: `rules.rs:50`, `lib.rs:268`
- **Validate**: `cargo test -p ferrobridge-analysis` (her açıklama × tip kombinasyonu için tablo testi; açıklamasız pointer hâlâ reddediliyor)

### Task 4: Kod üretimi
- **Action**: `&mut T` (primitif) ve `CxxVector<T>` Rust/C++ yazımları; shim'de `&param` ile adres geçirme; `OwnedPointer` → `return ::std::unique_ptr<T>(CALL);`; `BorrowedPointer` → null kontrolü, null'da `throw ::std::runtime_error(...)`, değilse `return *p;`; değerle dönen `std::vector` → `make_unique`. Üretilen dosyaların başına "Açıklamaya dayanan varsayımlar" listesi.
- **Mirror**: `shim.rs:86`, `spelling.rs:16`
- **Validate**: `cargo test -p ferrobridge-codegen` + snapshot incelemesi

### Task 5: CLI: `--config` ve `suggest`
- **Action**: `generate --config <yol>`; açıklama hataları çıkış kodu 1. `suggest <header> [--config]`: kalan açıklanmamış yerleri seçenekleri ve anlamlarıyla birlikte TOML olarak stdout'a basar; hiç yoksa "açıklama gereken yer yok" der.
- **Mirror**: `main.rs:36`
- **Validate**: `cargo test -p ferrobridge-cli` (`suggest` çıktısı geçerli TOML ve `generate --config`'e verilince ilgili uyarılar kaybolur)

### Task 6: Uçtan uca testler
- **Action**: `ownership` fixture'ı canlı nesne sayacı tutar (constructor artırır, destructor azaltır). Testler: `"owned"` dönen nesne `UniquePtr` düşünce gerçekten siliniyor (sayaç 0'a dönüyor); `"borrowed"` dönüş nesneyi silmiyor; null `"borrowed"` dönüş `Err`; `&mut f64` çıkış parametresi; ödünç pointer parametresi. `vectors`: Rust'tan vector oluşturup göndermek, geri almak, yerinde değiştirmek.
- **Mirror**: `crates/ferrobridge-e2e/tests/classes.rs`
- **Validate**: `cargo test -p ferrobridge-e2e`

### Task 7: İnceleme, ADR ve belgeler
- **Action**: `/ecc:security-review` + `/ecc:cpp-review` (odak: sahiplik devrinin doğruluğu, null kontrolleri, `&mut` aliasing); ADR-0006; README'ye `ferrobridge.toml` ve `suggest` bölümü; PRD (M3 durumu, açık soru kapanışı), plan notları, CLAUDE.md, Obsidian.
- **Validate**: README örnekleri kopyalanıp çalıştırıldığında çalışıyor

## Validation
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p ferrobridge-cli -- suggest tests/fixtures/classes/input.h
cargo run -p ferrobridge-cli -- generate tests/fixtures/ownership/input.h --out-dir target/gen \
  --config tests/fixtures/ownership/ferrobridge.toml
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| Kullanıcı yanlış açıklama yazar (`owned` dediği nesne `new` ile ayrılmamış) → UB | Orta | Güven modeli: söz tek yerde, varsayımlar listesi her dosyada, README'de net uyarı; ucuz kontroller (null) her zaman |
| cxx `CxxVector<OpakSınıf>` veya `&mut f64` için beklenmedik kısıt | Orta | Önce cxx denemesi; desteklenmeyen kombinasyon analizde açıklamalı reddedilir |
| Açıklama anahtarları overload'ları ayırt edemiyor | Orta | M3'te tüm overload'lara uygulanır ve bilinmeyen parametre hata verir; imzalı anahtar (`"geo::f(int*)"`) v0.2 |
| `borrowed` dönüşlerin null kontrolü exception kullanıyor; `-fno-exceptions` projelerinde derlenmez | Orta (ArduPilot'ta yüksek) | M2 ile aynı varsayım; PRD açık sorusu M5'ten önce karara bağlanacak |
| `std::vector<bool>` C++'ta özel (bit paketli), cxx desteklemiyor | Yüksek | Açıkça reddedilir ve test edilir |
| `"owned"` ile dönen arayüz pointer'ı aslında bir alt sınıfı gösteriyor, destructor sanal değil → `delete` UB | Orta | Karar 4: polimorfik sınıfta sanal destructor yoksa reddedilir; e2e'de sanal destructor'lı fabrika testi |
| Kapsam büyüklüğü (Large) | Orta | Null olabilen parametreler, slice'lar, C string'ler, `'static` dönüşler bilinçli olarak dışarıda |

## Acceptance
- [x] Açıklanmamış pointer'lar için hâlâ kod üretilmiyor; `suggest` bunlar için doldurulacak bir TOML taslağı basıyor
- [x] `ownership` fixture'ında `"owned"` dönen nesne Rust'ta `UniquePtr` düşünce C++'ta siliniyor (canlı nesne sayacıyla kanıtlı); `"borrowed"` dönüş silinmiyor; null ödünç dönüş `Err`
- [x] `double& out` gibi çıkış parametreleri açıklamayla `&mut f64` olarak çalışıyor
- [x] Public olmayan veya silinmiş destructor'lı sınıflar için `UniquePtr` üretilmiyor; polimorfik sınıfta sanal destructor yoksa `"owned"` açıklamalı olarak reddediliyor
- [x] `std::vector<T>` uçtan uca çalışıyor; `std::vector<bool>` açıklamalı olarak reddediliyor
- [x] Yazım hatalı/bilinmeyen açıklamalar satır numaralı hata veriyor
- [x] Üretilen dosyalar "Açıklamaya dayanan varsayımlar" listesini içeriyor
- [x] Kullanıcı tarafı testlerde hiç `unsafe` yok; validation komutlarının hepsi geçiyor
- [x] ADR-0006 yazıldı; güvenlik ve C++ incelemesi yapıldı
- [x] Patterns mirrored, not reinvented

## Uygulama notları
### cxx denemesi (2026-11-27, cxx 1.0.205, repo dışında)
Hepsi çalıştı: `&mut f64` parametresi (C++ `double&`); `&CxxVector<i32>`, `Pin<&mut CxxVector<f64>>`; `UniquePtr<CxxVector<CxxString>>` ve `UniquePtr<CxxVector<OpakSınıf>>` dönüşü (`get(i)`, `len()`, `iter()`); Rust'ta `CxxVector::<i32>::new()` + `push`; shim üzerinden `Result<&T>` ve `Result<Pin<&mut T>>` (null → `throw` → `Err`); `UniquePtr<T>(new T)` düşünce destructor çalışıyor (canlı nesne sayacı 0'a dönüyor).

**Bulgu:** Ödünç dönüş shim'i serbest fonksiyon olduğundan Rust'ın `self` ömür kuralı işlemez. Başka bir referans parametresi varsa `missing lifetime specifier` hatası çıkar. Shim imzası açık ömürle yazılmalı: `fn Widget_pick<'a>(fb_self: &'a Widget, other: &Widget) -> Result<&'a Widget>;`. Task 4'te ödünç dönüşlü her shim `'a` kullanır.

### Sonuç (2026-11-27)
Bütün görevler tamamlandı. Toplam 116 test (IR 6, parser 11, analiz 48, codegen 10, CLI 13 + parse, e2e 24). Homebrew libclang 23.1.6 ve Apple libclang 21.0.0 aynı sonucu veriyor. `fmt`, `clippy -D warnings` temiz; kullanıcı tarafı testlerde `unsafe` yok.

### Sapmalar ve bulgular
- **Destructor kontrolü (karar 4)** üç yerde ayrı uygulanıyor. Sanal destructor yalnızca `"owned"` pointer dönüşlerinde aranıyor, çünkü yalnızca orada nesnenin gerçek tipi bir alt sınıf olabilir. Constructor fabrikası, değerle dönüş ve `std::vector` elemanında tip kesin bilindiği için yalnızca public/silinmemiş destructor aranıyor. Tanımı görülemeyen taban sınıf "polimorfik, sanal destructor'ı yok" sayılıyor (en kısıtlayıcı varsayım). Alanların silinmiş destructor'ları dikkate alınmıyor; bu UB'ye değil derleme hatasına yol açar.
- **`void*` artık "açıklama gerekiyor" demiyor.** Açıklama onu bağlanabilir yapmadığı için doğrudan nedeni söyleniyor. `char*` (C string) ve `T**` için de aynısı geçerli. `suggest` yalnızca açıklamayla çözülebilecek yerleri basıyor. Bir fonksiyonun açıklamayla çözülemeyen başka bir sorunu varsa (ör. `long`) o fonksiyon için öneri yok.
- **`suggest` taslağındaki değerler yorum satırı.** Plandaki "suggest çıktısı `generate --config`'e verilince uyarılar kaybolur" ölçütü, test içinde satırlar açılarak doğrulanıyor. Değerleri dolu basmak sahipliği tahmin etmek olurdu (ADR-0006, alternatif 3).
- **Overload'lar:** Plandaki "bir overload'da olmayan parametre adı hata verir" kuralı "**hiçbir** overload'da olmayan parametre adı hata verir" olarak uygulandı. Aksi halde parametre adları farklı overload'lar hiç açıklanamazdı. Güvenlik incelemesinde ek bir önlem çıktı: bir açıklama birden fazla overload'a uygulanırsa, imzaları listeleyen bir uyarı basılıyor (biri pointer'ı saklıyor olabilir).
- **Geçersiz açıklama fixture'ı (`tests/fixtures/bad_config/`) açılmadı.** Bu durumlar analizde tablo testleriyle, CLI'da test içinde yazılan geçici dosyalarla sınanıyor.
- **`'a` ömrü:** Ödünç dönüşlü bütün metot shim'leri, başka referans parametresi olmasa da açık `'a` kullanıyor. Kural böylece tek ve basit kaldı.
- **`cargo fmt` ile üretici çakışıyordu.** rustfmt, commit'li `bridge.rs`'i prettyplease'ten farklı biçimlendirip senkron testini bozuyordu; M2'de satırlar kısa olduğu için görünmemişti. Üretilen `ffi` modülü artık `#[rustfmt::skip]` taşıyor. Bu, kullanıcı projelerinde de aynı sorunu önler.
- **cxx-insta:** `cargo-insta` kurulu değil. Snapshot'lar `INSTA_FORCE_PASS=1 INSTA_UPDATE=new` ile `.snap.new` olarak yazılıp elle incelendi, sonra `INSTA_UPDATE=always` ile kabul edildi.
- **Açık kalanlar:** Null olabilen parametreler (`Option`), pointer+uzunluk → slice, serbest fonksiyonlardan `'static` referans dönüşleri, imzalı (overload'a özgü) açıklama anahtarları, header içi attribute'lar (v0.2). `-fno-exceptions` sorusu M5'ten önce karara bağlanmalı: ödünç dönüşün null kontrolü de exception kullanıyor.
