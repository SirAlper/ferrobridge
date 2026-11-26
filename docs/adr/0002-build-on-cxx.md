# ADR-0002: Arka uç olarak `cxx` üstüne inşa etmek

**Tarih**: 2026-11-26 (karar aynı gün rakip analizinden sonra verildi; bu kayıt sonradan yazıldı)
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

ferrobridge'in ürettiği kodun Rust ile C++ arasındaki çağrıları güvenli biçimde taşıması gerekiyor. Bu katmanı (FFI, ABI uyumu, `std::string`/`std::vector`/`std::unique_ptr` köprüleri, exception yakalama) sıfırdan yazmak hem uzun sürer hem de UB riskini artırır. Rakip analizine göre (Kasım 2026) cxx olgun, Chromium ve Android'de kullanılıyor; Google, Crubit'e geçse bile cxx'i kullanımdan kaldırmayacağını açıkladı (Temmuz 2026).

## Karar

ferrobridge kendi FFI katmanını yazmıyor. Üretilen kod, `#[cxx::bridge]` modülleri ve cxx'in std tip köprüleri üzerine kurulu. ferrobridge'in katkısı bunların **üstünde**: header'dan otomatik üretim, sahiplik açıklamaları ve güvenli, idiomatik sarmalayıcı API.

## Değerlendirilen alternatifler

### Alternatif 1: Kendi FFI katmanımızı yazmak (`extern "C"` shim'ler + ham pointer'lar)
- **Artıları**: Tam kontrol, cxx'in kısıtlarına bağlı kalmamak.
- **Eksileri**: Çok iş; std tip köprüleri, exception yönetimi ve ABI ayrıntıları sıfırdan yazılır. UB riski yüksek.
- **Neden seçilmedi**: Farkımız FFI katmanında değil, üretim ve güvenlik katmanında.

### Alternatif 2: Crubit'in çalışma zamanı parçalarını kullanmak
- **Artıları**: Daha zengin tip desteği hedefliyor.
- **Eksileri**: Bazel'e bağlı, Cargo desteği henüz yok, birçok özelliği deneysel.
- **Neden seçilmedi**: Hedef kullanıcımız Cargo/CMake kullanıyor.

### Alternatif 3: Zngur
- **Artıları**: Generic ve trait desteği güçlü, her build sistemiyle çalışıyor.
- **Eksileri**: Ağırlıklı olarak ters yönü (C++'ın Rust'ı kullanmasını) çözüyor.
- **Neden seçilmedi**: MVP yönümüz Rust → C++. Zngur ileride birlikte çalışma hedefi.

## Sonuçlar

### Olumlu
- Güvenilir, yaygın kullanılan bir temel; std tip köprüleri ve exception → `Result` hazır.
- Kullanıcılar cxx'i tanıyorsa üretilen kod tanıdık görünür.

### Olumsuz
- cxx'in kısıtları bizim de kısıtımız olur: opak tipler, `Pin<&mut T>`, `UniquePtr` gereksinimi. Güvenli sarmalayıcı katmanı bunları kullanıcıdan gizlemek zorunda.
- cxx'in desteklemediği bir yapı için ek shim gerekir.

### Riskler
- **cxx'in gelişimi durur veya yön değiştirir.** Önlem: Üretim katmanı IR'dan çalıştığı için başka bir arka uç eklenebilir; 3 ayda bir rakip kontrolünde cxx'in durumu da izlenir.
