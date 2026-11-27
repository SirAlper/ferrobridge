# ferrobridge

C++ header'larından **güvenli ve idiomatik** Rust binding'leri üreten araç. cxx üzerine kuruludur, sahiplik bilgisini bir açıklama dosyasından (`ferrobridge.toml`) alır ve Bazel gerektirmez.

> Durum: erken geliştirme (Milestone 6 / v0.1 öncesi). Henüz yayınlanmış bir sürüm yok. Gerçek dünya demosu: ArduPilot `AP_Math` ([`demo/ap_math/`](demo/ap_math/README.md)).

## Gereksinimler

- Rust (edition 2024, en az 1.85)
- LLVM / libclang **16 veya üstü**
- C++17 derleyicisi (üretilen kodu derlemek için)

```bash
brew install llvm
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

Ortamı kontrol etmek için:

```bash
cargo run -p ferrobridge-cli -- doctor
```

## Kullanım

### Bridge üretmek

```bash
cargo run -p ferrobridge-cli -- generate include/vehicle.h --out-dir src/ffi \
  --include-as vehicle.h --shim-namespace ferrobridge::vehicle -- -I include
```

`src/ffi/` altına üç dosya yazılır:

| Dosya | İçerik |
|---|---|
| `bridge.rs` | Güvenli Rust API'si (sarmalayıcılar, metotlar, gerçek `enum`'lar, serbest fonksiyonlar) ve altındaki ham `#[cxx::bridge] pub mod ffi` |
| `shim.h`, `shim.cc` | cxx'in doğrudan çağıramadıkları için C++ yardımcıları: constructor fabrikaları (`Vehicle_new_i32`), statik metotlar, değerle dönen sınıf/`std::string`/`std::vector` sarmalayıcıları, açıklamalı pointer'lar ve bütün C++ exception'larını `Err`'e çeviren işleyici |

Bağlanamayan her öğe uyarı olarak basılır ve üretilen dosyaların başında nedeniyle listelenir; hiçbir şey sessizce atlanmaz. Üretilen dosyalar commit'lenir ve elle düzenlenmez (ADR-0004). `generate`, ferrobridge'in üretmediği bir dosyanın üzerine `--force` olmadan yazmaz.

Üretilen `bridge.rs`'i bir modül olarak dahil edin ve crate'inize `cxx` ile `ferrobridge` (çalışma zamanı: ortak hata tipi) bağımlılıklarını ekleyin:

```rust
#[path = "ffi/bridge.rs"]
pub mod vehicle;
```

C++ tarafını `build.rs`'te `ferrobridge-build` derler (`[build-dependencies]`). `ferrobridge-e2e/build.rs` aynı yolu kullanır:

```rust
// build.rs
fn main() {
    let mut build = ferrobridge_build::Build::new();
    build
        .committed("src/ffi")         // bridge.rs, shim.h, shim.cc
        .include_dir("include")       // `--include-as vehicle.h` buradan çözülür
        .include_dir("src/ffi")       // bridge'in `include!("shim.h")`'si (varsayılan `--shim-include`)
        .file("cpp/vehicle.cc");      // kütüphanenin gerçeklemesi
    if let Err(error) = build.compile("vehicle") {
        panic!("{error}");
    }
}
```

Bir bridge birden fazla header'dan üretilebilir: `ferrobridge generate a.h b.h ...` (header başına bir `--include-as`). Bir header'da tanımlı tip diğerlerinin fonksiyonlarında kullanılabilir. Bridge'e eklenmemiş bir header'daki tip atlanır, mesaj hangi header'ı ekleyeceğinizi söyler (ADR-0009).

Header kendi başına derlenmiyorsa (başka bir header'dan önce include edilmesi gerekiyorsa) `--prelude <header>` verin; hem libclang hem shim onu önce include eder.

Üretilmiş dosyaları commit'leyemiyorsanız (ör. kütüphanenin lisansı yüzünden), `ferrobridge-build`'in `generate` özelliği header'ı derleme sırasında okuyup bridge'i `OUT_DIR`'e üretir. Bu mod her derlemede libclang ister (ADR-0008):

```rust
// build.rs (ferrobridge-build = { ..., features = ["generate"] })
build.generate(
    ferrobridge_build::Generate::new("include/vehicle.h", "vehicle")
        .include_as("vehicle.h")
        .config("ferrobridge.toml"),
);

// src/lib.rs
pub mod vehicle {
    ferrobridge::include_bridge!("vehicle");
}
```

Rust tarafında hiç `unsafe`, `Pin` ya da cxx tipi gerekmez (gerçek fixture'larla derlenen kopyası: `crates/ferrobridge-e2e/tests/readme.rs`):

```rust
use vehicle::{Vehicle, Mode};

fn main() -> ferrobridge::Result<()> {
    let mut car = Vehicle::new_i32(4)?;           // C++ constructor; sahip olan sarmalayıcı
    car.drive(12.5)?;                             // &mut self; C++ exception → Err
    let km = car.odometer();                      // noexcept → doğrudan değer
    let text: String = vehicle::greet("Ada")?;    // &str girer, String çıkar
    match vehicle::next(Mode::Auto)? {            // gerçek Rust enum'u, kapsam denetimli
        Mode::Manual | Mode::Auto | Mode::Hybrid => {}
    }
    Ok(())
}
```

| C++ | Güvenli API |
|---|---|
| Constructor, değerle dönen sınıf | Sahip olan sarmalayıcı `T` (düşünce C++ destructor'ı çalışır) |
| Bütün alanları public, bit bit kopyalanabilen sınıf (`struct Pt { double x, y; }`) | Değer tipi: `#[repr(C)]` Rust struct'ı, alanlar doğrudan, değerle geçer, metotlar `&self`/`&mut self`. Düzen C++'ta `static_assert` ile doğrulanır (ADR-0010) |
| Üye operatörler (`+ - * / %`, `+=`..., `==`, tekli `-`) | Değer tiplerinde `std::ops` trait'leri (hata dönmüyorsa), her durumda adlı metotlar (`op_add`). `operator[]` bağlanmaz: sınır denetimi yok |
| Sınıf template'i (`[templates]` ile örneklenmiş) | Örneklemesi (`Vector3f`), diğer sınıflar gibi; değer tipi de olabilir |
| `owned` pointer, `std::unique_ptr<T>` dönüşü | `Option<T>` |
| Metotlar | `&self` / `&mut self`; ödünç nesnelerde (`&ffi::T`, `Pin<&mut ffi::T>`) aynı metotlar |
| `const std::string&` / `std::string` dönüşü | `&str` / `String` (geçersiz UTF-8 → `Err`) |
| `const std::vector<T>&` (T sayı) / değerle dönen vector | `&[T]` / `Vec<T>`, `Vec<String>` (kopyalanır) |
| `enum` | Gerçek Rust `enum`'u; C++'tan tanımsız değer → `Err` |
| `const`/`constexpr` sayı ve `bool` sabitleri | `pub const` (değer üretim anında hesaplanır; tamsayılar C++ derlemesinde `static_assert` ile yeniden doğrulanır) |
| Tanımı header'da olan `static inline` fonksiyon | Sıradan güvenli fonksiyon |
| Exception fırlatabilen her şey | `ferrobridge::Result<T>`; hata C++ öğesinin adını taşır (`geo::Vehicle::break_down: motor arızası`) |

Değiştirilebilir `std::string&` / `std::vector<T>&` ve sınıf elemanlı vector'ler cxx tipleri olarak kalır: kopyala-geri yaz, C++ referansı saklıyorsa sessizce yanlış olurdu. Ham cxx katmanı `vehicle::ffi` altında kaçış kapısı olarak durur (metotlar `raw_<ad>`). Sarmalayıcıyı `&ffi::T` bekleyen bir fonksiyona `&araba` diye verebilirsiniz (deref coercion); bunun için değer bir değişkene bağlı olmalı. Tasarım: ADR-0007.

Adlandırma kuralları (overload'lar için `clamp_i32_i32_i32`, namespace çakışmaları için `geo_length`, constructor'lar için `Vehicle::new_i32`) ADR-0005'te. Gerçek bir örnek için `crates/ferrobridge-e2e/`.

### Sahiplik açıklamaları (`ferrobridge.toml`)

C++ header'ı bir pointer'ın sahibini ve ömrünü söylemez; ferrobridge bunu **tahmin etmez**. Ham pointer (`T*`) ve const olmayan primitif referans (`double& out`) içeren öğeler, siz açıklayana kadar bağlanmaz. Açıklama bekleyen yerler için taslak:

```bash
cargo run -p ferrobridge-cli -- suggest include/vehicle.h > ferrobridge.toml
```

Taslaktaki değerler yorum satırıdır. C++ kütüphanesini bilerek, emin olduğunuz satırları açın:

```toml
# Dönen pointer `new` ile ayrıldı; silmek çağırana düşüyor.
[functions."geo::make_widget"]
returns = "owned"

# Dönen pointer nesnenin sahip olduğu bir şeyi gösteriyor; nesne yaşadıkça geçerli.
[functions."geo::Widget::child"]
returns = "borrowed"

# Yalnızca çağrı süresince kullanılıyor, saklanmıyor, null değil, tek bir nesne (dizi değil).
[functions."geo::Widget::attach".params]
target = "borrowed"

[functions."geo::Vehicle::read_odometer".params]
out = "borrowed"

# `p`, `len` parametresi kadar elemanlık bir dizi; yalnızca çağrı süresince kullanılıyor.
[functions."crc_crc8".params]
p = { slice = "len" }

# Sınıf template'lerini somut tiplerle örnekle (kütüphanenin gerçekten örneklediği tipler).
[templates]
Vector3f = "Vector3<float>"

# Kütüphane exception fırlatmıyor (ör. `-fno-exceptions`): fonksiyonlar `Result` yerine
# doğrudan değer döndürür. Söz bozulursa program sonlanır, tanımsız davranış olmaz (ADR-0011).
[library]
exceptions = false
```

```bash
cargo run -p ferrobridge-cli -- generate include/vehicle.h --out-dir src/ffi --config ferrobridge.toml
```

| C++ | Açıklama | Rust |
|---|---|---|
| `T* f()` (T: sınıf, `std::string`, `std::vector`) | `returns = "owned"` | `UniquePtr<T>` (null → boş `UniquePtr`) |
| `T* m()` (metot) | `returns = "borrowed"` | `Result<&T>` / `Result<Pin<&mut T>>`, ömrü nesneye bağlı; null → `Err` |
| `double& m()` (metot) | `returns = "borrowed"` | `&mut f64` |
| `f(const T* p)` / `f(T* p)` | `p = "borrowed"` | `&T` / `Pin<&mut T>` (primitifte `&mut f64`) |
| `f(double& out)` | `out = "borrowed"` | `&mut f64` |
| `f(const uint8_t* p, uint8_t len)` / `f(float* p, size_t n)` | `p = { slice = "len" }` | `f(p: &[u8])` / `f(p: &mut [f32])`; uzunluk Rust imzasından kalkar. Slice C++ uzunluk tipine sığmazsa `Err` (sessiz kırpma yok) |

Boş bir Rust slice'ı C++'a null olmayan (ama okunmaması gereken) bir pointer ve `0` uzunlukla gelir; C++ `p == nullptr`'a ayrıca bakıyorsa farklı davranabilir.

Tanımı header'da olan `static` / `static inline` fonksiyonlar bridge'in kendi çeviri biriminde derlenir. Gövdelerinde `static` yerel değişken varsa Rust'tan yapılan çağrılar, C++ kütüphanesinin kendi kopyasından ayrı bir duruma bakar (tanımsız davranış değil, ama durum paylaşılmaz).

Anahtar tam C++ adıdır (constructor için `ns::Sınıf::Sınıf`; isimsiz parametreler `arg0`, `arg1`...) ve aynı adlı **bütün overload'lara** uygulanır. Bu durumda bir uyarı basılır.

> ⚠️ ferrobridge, C++ fonksiyonlarının ön koşullarını bilemez: ön koşulu bozulunca tanımsız davranışa giren bir C++ fonksiyonu, güvenli Rust'tan çağrılınca da girer. Bu, bütün bağlama araçlarının sınırı. ferrobridge pointer ve uzunlukları slice yapar, null dönüşleri `Err` yapar, denetimsiz `operator[]`'yi bağlamaz.

> ⚠️ Her açıklama, C++ kütüphanesi hakkında verdiğiniz bir **sözdür**, tıpkı Rust'taki `unsafe` bloğu gibi. Söz yanlışsa (ör. `owned` dediğiniz nesne `new` ile ayrılmamışsa) üretilen kod tanımsız davranışa yol açabilir. Kullanılan her söz, üretilen dosyaların başında "Açıklamaya dayanan varsayımlar" olarak listelenir. ferrobridge ucuz olan kontrolleri yine de yapar. Ödünç dönüş null gelirse referans yerine `Err` döner. Destructor'ı public olmayan sınıflar için `UniquePtr` üretilmez. Polimorfik bir sınıfın destructor'ı sanal değilse `owned` reddedilir.

Header'da karşılığı olmayan açıklamalar (yazım hatası, olmayan parametre) satır numarasıyla hata verir. Açıklama gerektirmeyen yere yazılan açıklama uyarı verir. Karar: ADR-0006.

### Header'ı incelemek

Bir header'ın içinde ne bulunduğunu görmek için:

```bash
cargo run -p ferrobridge-cli -- parse tests/fixtures/free_functions/input.h
```

Çıktı, header'daki fonksiyonları, sınıfları ve enum'ları içeren girintili JSON'dur (IR). Henüz desteklenmeyen yapılar (template, union...) atlanmaz; `"kind": "unsupported"` öğesi olarak nedenleriyle listelenir. clang'a ek argüman vermek için `--` kullanın:

```bash
cargo run -p ferrobridge-cli -- parse include/mylib.h --std c++20 -- -I include -DMYLIB_EXPORT=
```

## Workspace

| Crate | Görev |
|---|---|
| `ferrobridge-ir` | Dilden bağımsız ara model (başka ferrobridge crate'ine bağımlı değil) |
| `ferrobridge-parser` | libclang ile C++ header okuma |
| `ferrobridge-analysis` | Neyin güvenle bağlanabileceğine karar verir (güvenlik kuralları), Rust adlarını atar |
| `ferrobridge-codegen` | Güvenli Rust API, cxx bridge ve C++ shim üretimi |
| `ferrobridge` | Üretilen kodun çalışma zamanı: `ferrobridge::Error` ve dönüşüm yardımcıları (yalnızca `cxx`'e bağlı, `unsafe` yok) |
| `ferrobridge-build` | `build.rs` içinden çağrılan API: commit'li bridge'leri ya da (`generate` özelliği) derleme sırasında üretilenleri derler |
| `ferrobridge-cli` | `ferrobridge` komut satırı aracı (`doctor`, `parse`, `generate`, `suggest`) |
| `ferrobridge-e2e` | Uçtan uca testler: üretilen bridge'ler gerçek C++ ile derlenip Rust'tan çağrılır |

`demo/ap_math/` kök workspace'in parçası değildir: ArduPilot'a bağlandığı için GPL-3.0-or-later lisanslı ayrı bir crate (ADR-0008).

Test fixture'ları `tests/fixtures/<konu>/input.h` altında, beklenen çıktılar `crates/*/tests/snapshots/` altında [insta](https://insta.rs) snapshot'ı olarak tutulur. Çıktı değiştiğinde `cargo insta review` ile inceleyin.

## Belgeler

- Gereksinimler: [`.claude/prds/ferrobridge.prd.md`](.claude/prds/ferrobridge.prd.md)
- Mimari kararlar: [`docs/adr/`](docs/adr/README.md)

## Lisans

ferrobridge açık kaynaktır ve iki lisansın birini seçebileceğiniz şekilde dağıtılır:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

### Katkı

Aksi açıkça belirtilmedikçe, Apache-2.0 lisansında tanımlandığı şekliyle bu projeye dahil edilmek üzere bilerek gönderdiğiniz her katkı, ek şart veya koşul olmadan yukarıdaki gibi çift lisanslanır.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
