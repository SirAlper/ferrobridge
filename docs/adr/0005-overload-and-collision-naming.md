# ADR-0005: Overload ve ad çakışması adlandırma kuralı

**Tarih**: 2026-11-27
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

C++ fonksiyon overload'una ve aynı adın farklı namespace'lerde bulunmasına izin verir; bir cxx bridge'inde ise Rust adları benzersiz olmalıdır (bridge'deki bütün öğeler tek bir Rust modülünde yaşar). Ayrıca C++ adları Rust anahtar kelimesi olabilir (`type`, `move`), ve cxx Rust parametre adlarını üretilen C++ koduna kopyaladığı için uydurulan adlar C++ anahtar kelimesi olmamalıdır (cxx denemesinde `this` adlı parametre derleme hatası verdi). Kuralın deterministik ve açıklanabilir olması gerekir.

## Karar

Rust adları sırasıyla şu kurallarla verilir; C++ adı her zaman `#[cxx_name]` ile korunur:

1. Tek tanımlı isim olduğu gibi kalır.
2. Aynı C++ adı birden fazla namespace'te varsa namespace yolu ön ek olur (`geo_length`, `physics_length`). Tipler için PascalCase (`PhysicsVehicle`).
3. Aynı adı taşıyan overload'ların **hepsi** parametre tiplerinden türetilen sonek alır (`clamp_i32_i32_i32`, `clamp_f64_f64_f64`; parametresiz için `noargs`; `Pin<&mut T>` için `mut_T`).
4. Constructor'lar `<Sınıf>_new`, statik metotlar `<Sınıf>_<metot>` olur (aynı kurallarla).
5. Rust anahtar kelimeleri `_` ile kaçılır (`type_`); isimsiz parametreler `argN` olur. Shim'lerdeki alıcı parametresi `fb_self` (C++ anahtar kelimesi değil).
6. Bunlardan sonra hâlâ çakışan adlara sıra numarası eklenir (`_2`).

## Değerlendirilen alternatifler

### Alternatif 1: İlk overload yalın kalır, diğerleri numaralanır (`clamp`, `clamp_2`)
- **Artıları**: Kısa adlar.
- **Eksileri**: Numara, overload'un ne olduğunu söylemez; header'daki sıra değişince anlam değişir.
- **Neden seçilmedi**: Tipten türetilen ad kendini açıklar ve sıradan bağımsızdır.

### Alternatif 2: Overload'ları hiç bağlamamak
- **Artıları**: Ad sorunu yok.
- **Eksileri**: Gerçek C++ API'lerinin büyük kısmı overload kullanır.
- **Neden seçilmedi**: PRD'nin kapsam hedefine (%80) ulaşılamaz.

### Alternatif 3: Kullanıcının her overload için ad vermesi
- **Artıları**: En okunur adlar.
- **Eksileri**: Açıklama dosyası olmadan çalışmaz; ilk kullanımda sürtünme.
- **Neden seçilmedi**: Varsayılan olarak otomatik kural, ileride `ferrobridge.toml` ile ad geçersiz kılma (M3/M4) birlikte en iyisi.

## Sonuçlar

### Olumlu
- Adlar deterministik ve tiplerden okunabilir.
- Snapshot testleri adları sabitler; beklenmeyen ad değişikliği diff'te görünür.

### Olumsuz
- **Bilinçli bedel:** Header'a yeni bir overload eklenince, önce tek olan fonksiyonun adı da değişir (`area` → `area_noargs`). Bu, kullanıcı kodunu kırar.
- Adlar Rust stiline uymayabilir (`Vehicle_new_i32`); üretilen modülde `non_snake_case` uyarısı kapatıldı. İdiomatik adlar M4'teki sarmalayıcı katmanın işi.

### Riskler
- **Ad kayması kullanıcı kodunu kırar.** Önlem: M4'teki güvenli API katmanı ve `ferrobridge.toml`'daki ad geçersiz kılma ile kullanıcıya sabit adlar verilecek.
