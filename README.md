# ferrobridge

C++ header'larından **güvenli ve idiomatik** Rust binding'leri üreten araç. cxx üzerine kuruludur, sahiplik bilgisini bir açıklama dosyasından (`ferrobridge.toml`) alır ve Bazel gerektirmez.

> Durum: erken geliştirme (Milestone 3 / v0.1 öncesi). Henüz yayınlanmış bir sürüm yok.

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
| `bridge.rs` | `#[cxx::bridge] pub mod ffi`: sınıflar (opak tip), metotlar, enum'lar, serbest fonksiyonlar |
| `shim.h`, `shim.cc` | cxx'in doğrudan çağıramadıkları için C++ yardımcıları: constructor fabrikaları (`Vehicle_new_i32`), statik metotlar, değerle dönen sınıf/`std::string`/`std::vector` sarmalayıcıları, açıklamalı pointer'lar ve bütün C++ exception'larını `Err`'e çeviren işleyici |

Bağlanamayan her öğe uyarı olarak basılır ve üretilen dosyaların başında nedeniyle listelenir; hiçbir şey sessizce atlanmaz. Üretilen dosyalar commit'lenir ve elle düzenlenmez (ADR-0004). `generate`, ferrobridge'in üretmediği bir dosyanın üzerine `--force` olmadan yazmaz.

Rust tarafında (cxx ve `cxx-build` ile derlendikten sonra) hiç `unsafe` gerekmez:

```rust
let mut vehicle = ffi::Vehicle_new_i32(4)?;   // C++ constructor
vehicle.pin_mut().drive(12.5)?;                  // C++ exception → Err
let km = vehicle.odometer();                     // noexcept → doğrudan değer
```

Adlandırma kuralları (overload'lar için `clamp_i32_i32_i32`, namespace çakışmaları için `geo_length`) ADR-0005'te. Gerçek bir örnek için `crates/ferrobridge-e2e/`.

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

Anahtar tam C++ adıdır (constructor için `ns::Sınıf::Sınıf`; isimsiz parametreler `arg0`, `arg1`...) ve aynı adlı **bütün overload'lara** uygulanır. Bu durumda bir uyarı basılır.

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
| `ferrobridge-codegen` | cxx bridge ve C++ shim üretimi |
| `ferrobridge-build` | `build.rs` içinden çağrılan API |
| `ferrobridge-cli` | `ferrobridge` komut satırı aracı (`doctor`, `parse`, `generate`) |
| `ferrobridge-e2e` | Uçtan uca testler: üretilen bridge'ler gerçek C++ ile derlenip Rust'tan çağrılır |

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
