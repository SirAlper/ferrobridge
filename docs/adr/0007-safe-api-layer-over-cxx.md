# ADR-0007: cxx katmanının üstünde güvenli API katmanı

**Tarih**: 2026-11-27
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

M2 ve M3'te üretilen cxx bridge'i güvenliydi ama idiomatik değildi. Kullanıcı `Vehicle_new_i32(4)`, `vehicle.pin_mut().drive(..)`, `let_cxx_string!`, `CxxVector` ve `cxx::Exception` ile uğraşıyordu. PRD'nin hipotezi "güvenli **ve idiomatik**" Rust API'si; `03 Mimari` tip tablosunda da bir "Güvenli API" sütunu var. C++ nesneleri Rust'ta üç biçimde görünür: sahip olunan (`UniquePtr<T>`), ödünç (`&T`) ve değiştirilebilir ödünç (`Pin<&mut T>`, M3'ün ödünç dönüşleri dahil). Güvenli API bu üçünde de çalışmalıdır. Güvenlik kuralı 3 gereği trivial olmayan C++ nesneleri Rust'ta değerle tutulamaz.

## Karar

`bridge.rs` cxx'in `ffi` modülünün yanına güvenli bir katman da içerir:
1. Güvenli metotlar cxx'in opak tipine inherent `impl ffi::T` olarak yazılır (`&self` / `self: Pin<&mut Self>`). cxx'in kendi metotları çakışmasın diye `raw_<ad>` olur; C++ adı `#[cxx_name]` ile korunur.
2. Bridge'de `UniquePtr<T>` olarak geçen her sınıf için sahip olan bir sarmalayıcı üretilir: `pub struct T { inner: UniquePtr<ffi::T> }`, içi asla null değil, `Deref<Target = ffi::T>`, değiştiren metotlar için `&mut self` ileticileri, `T::new*` constructor'ları ve statik metotlar.
3. Bütün hatalar yeni çalışma zamanı crate'i `ferrobridge`'deki tek bir `ferrobridge::Error` tipindedir; hata, geldiği C++ öğesinin adını taşır.
4. Dönüşümler: `&str` / `String`, sayı slice'ı / `Vec<T>`, `Vec<String>`, gerçek Rust `enum`'ları (`TryFrom`), null olabilen sahip dönüşler için `Option<T>`. Değiştirilebilir string/vector referansları ve sınıf elemanlı vector'ler olduğu gibi kalır.

## Değerlendirilen alternatifler

### Alternatif 1: Yalnızca sarmalayıcıda güvenli metotlar + ödünç nesneler için `TRef<'a>` / `TMut<'a>` görünüm tipleri
- **Artıları**: `ffi` katmanına hiç dokunulmaz.
- **Eksileri**: Her sınıf için üç tip ve her metot üç kez; M3'ün ödünç dönüşleri görünüm tipine sarılmalı; kullanıcı aynı nesne için üç ad öğrenir.
- **Neden seçilmedi**: Opak tipe inherent `impl` yazmak cxx'te çalışıyor (deneme ile doğrulandı) ve tek bir metot tanımıyla üç biçimi de kapsıyor.

### Alternatif 2: Sarmalayıcı yok, kullanıcı `UniquePtr<ffi::T>` ve `pin_mut()` kullanır
- **Artıları**: En az üretilen kod.
- **Eksileri**: `vehicle.pin_mut().drive(..)` idiomatik değil; constructor'lar serbest fonksiyon kalır.
- **Neden seçilmedi**: M4'ün amacı tam olarak bu sürtünmeyi kaldırmak.

### Alternatif 3: Her header'a ayrı bir hata tipi üretmek (çalışma zamanı crate'i yok)
- **Artıları**: Kullanıcı crate'ine ek bağımlılık yok.
- **Eksileri**: İki header'ın hataları farklı tipte olur, `?` ile birleşmez; ortak dönüşüm kodu her dosyada tekrarlanır.
- **Neden seçilmedi**: Küçük, `unsafe` içermeyen, yalnızca `cxx`'e bağlı bir crate daha ucuz.

### Alternatif 4: Değiştirilebilir `std::string&` / `std::vector<T>&` için kopyala-geri yaz (`&mut String`)
- **Artıları**: Daha idiomatik imza.
- **Eksileri**: C++ referansı saklıyorsa kopya sessizce eskir; anlam değişir.
- **Neden seçilmedi**: Güvenlik kuralı 1 (tahmin etme). Bu yüzden ham `Pin<&mut CxxString>` kalır.

## Sonuçlar

### Olumlu
- `let mut v = Vehicle::new_i32(4)?; v.drive(12.5)?;`: `Pin`, cxx tipi ya da `unsafe` gerekmiyor (e2e testlerinde otomatik denetleniyor).
- Geçersiz UTF-8 ve C++'tan gelen tanımsız enum değeri panik değil `Err`.
- Ham `ffi` katmanı kopyasız kullanım ve güvenli katmanın çevirmediği tipler için kaçış kapısı olarak duruyor.

### Olumsuz
- cxx metotları `raw_*` olarak yeniden adlandırıldı (v0.1 öncesi, dış kullanıcı yok).
- Kullanıcı crate'i `ferrobridge` çalışma zamanı crate'ine bağımlı.
- `&[T]` parametreleri ve `String`/`Vec` dönüşleri her çağrıda kopyalanır.
- Sarmalayıcıdan `&ffi::T` beklenen yere deref coercion, değer bir değişkene bağlıyken çalışır; `f(&g()?)` biçiminde çalışmaz.
- Enum döndüren `noexcept` bir fonksiyon da `Result` döner (tanımsız değer ihtimali).

### Riskler
- C++ metodu sarmalayıcının yardımcısıyla (`pin_mut`, `into_inner`, `from_unique_ptr`) aynı adı taşırsa, sarmalayıcıda iletici `_` sonekiyle üretilir; const metot için Deref yerine sarmalayıcınınki seçilir. Önlem: belgelendi; C++ metoduna `(*w).ad()` ile ulaşılır.
- `ffi` adlı bir C++ sınıfı modül adıyla çakışır. Önlem: v0.1'de bilinen sınırlama.
