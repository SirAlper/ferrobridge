# ADR-0011: Exception kullanmayan kütüphaneler (`[library] exceptions = false`)

**Tarih**: 2026-11-27
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

4. güvenlik kuralı gereği exception'lar FFI sınırını geçmez: `noexcept` olmayan her C++ fonksiyonu Rust'ta `Result` döndürür. Gömülü kütüphaneler çoğu zaman `-fno-exceptions` ile derlenir ve fonksiyonlarına `noexcept` yazmaz. Örneğin ArduPilot'un waf ayarları bunu kullanıyor ve `AP_Math`'te `throw` yok. Böyle bir kütüphanede her çağrının `Result` dönmesi (`v.length()?`) API'yi gürültülü yapar ve bilgi taşımaz. PRD'de bu bir açık soruydu.

## Karar

`ferrobridge.toml`'da şu bir sözdür: "bu kütüphanenin hiçbir fonksiyonu exception fırlatmaz".

```toml
[library]
exceptions = false
```

- Söz verilirse bütün fonksiyonlar `noexcept` sayılır ve doğrudan değer döndürür.
- ferrobridge'in kendi kontrolleri `Result` olarak kalır: slice uzunluk taşması, `borrowed` null dönüşü, tanımsız enum değeri, UTF-8.
- **Söz bozulursa tanımsız davranış olmaz.** cxx, `Result` döndürmeyen fonksiyonların C++ sarmalayıcısını `noexcept` yapar; shim de `noexcept` olur. Bir exception fırlatılırsa program `std::terminate` ile sonlanır. Bu, M6 Task 0'da ve e2e'de bir alt süreçle doğrulandı.
- Söz, üretilen dosyaların "Açıklamaya dayanan varsayımlar" listesine girer.

## Değerlendirilen alternatifler

### Alternatif 1: Fonksiyon başına `noexcept = true` açıklaması
- **Neden seçilmedi**: Kütüphane genelinde doğru olan bir özellik için yüzlerce satır gerektirir. İleride istisnalar için eklenebilir.

### Alternatif 2: `-fno-exceptions` derleme bayrağını otomatik algılamak
- **Eksileri**: Bayrak kütüphanenin kendi derlemesine aittir. Bridge'in derlemesinden çıkarılamaz, kesin değildir.
- **Neden seçilmedi**: Tahmin olurdu (1. kural).

## Sonuçlar

### Olumlu
- Demoda `a.length()`, `ap::degrees(x)` gibi çağrılar `?` olmadan.
- `std::ops` trait'leri de bu kütüphanelerde üretilebiliyor; trait metotları `Result` döndüremez.

### Olumsuz
- Söz yanlışsa program bir hata dönmek yerine sonlanır. Bu, UB yerine bilinçli olarak seçilmiş güvenli bir başarısızlık.
