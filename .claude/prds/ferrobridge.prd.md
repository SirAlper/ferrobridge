# ferrobridge — Ürün Gereksinim Dokümanı (PRD)

> Bu dosyanın asıl kopyası Obsidian'da: `rust/ferrobridge/06 PRD.md`. İki kopya aynı tutulmalı.
> Tarih: 26 Kasım 2026

## Problem
Elinde büyük C++ kod tabanı olan ve yeni kodu Rust'ta yazmak isteyen ekiplerdeki geliştiriciler, her C++ fonksiyonu ve sınıfı için köprü (bridge) kodunu elle yazmak zorunda kalıyor. Var olan otomatik araç ya bakım modunda ya da yalnızca Bazel ile çalışıyor. C++ header'ında sahiplik ve ömür bilgisi bulunmadığı için geliştirici bunu tahmin ediyor. Yanlış tahmin tanımsız davranışa (UB) yol açabiliyor. Sorun çözülmezse ekipler Rust'a geçişi erteliyor ya da güvensiz, elle yazılmış ve bakımı zor ara katmanlarla yaşıyor.

## Evidence
**Ekosistem seviyesinde (Kasım 2026 araştırması):**
- Rust Foundation'ın Rust/C++ Interop Initiative'i Eylül 2026 raporunda 40 interop problemi listeliyor. Harici kütüphaneler dışında çözüm üretiminde "relatively little progress" olduğunu belirtiyor.
- Google, Chromium'da Crubit'e yatırım yapıyor (Temmuz 2026). Büyük kuruluşlar aynı sorunu yaşıyor.
- autocxx resmî olarak bakım modunda. Crubit'e göre kendisi "not yet viable for general purpose use" ve yalnızca Bazel destekliyor; CMake desteği planlanmıyor.
- Pek çok şirket bindgen üstünde kendi iç çözümünü yazıyor (Rust Foundation raporu). Bu, elle yapılan geçici çözümlere işaret ediyor.

**Kullanıcı seviyesinde:**
- Varsayım: Hedef geliştiricilerin bu acıyı yaşadığı ve otomatik bir araç kullanacağı varsayılıyor. Doğrulama yolları: (1) autocxx ve cxx GitHub issue'larının taranması, (2) ArduPilot/PX4 forumlarında Rust tartışmalarının incelenmesi, (3) gerçek dünya demosunun topluluğa gösterilip geri bildirim toplanması.

## Users
- **Birincil:** Mevcut bir C++ kütüphanesini Rust'tan kullanması gereken ve **Cargo** (ileride CMake) kullanan Rust geliştiricisi. Örnek alanlar: robotik/İHA, gömülü sistemler, oyun motorları, simülasyon. İhtiyaç, yeni bir Rust modülünün eski C++ koduna erişmesi gerektiğinde ortaya çıkıyor.
- **Hedef değil:**
  - Bazel kullanan ekipler (Crubit bunu çözüyor)
  - Rust kodunu C++'tan çağırmak isteyenler (Zngur bunu çözüyor)
  - Saf C kütüphanesi kullananlar (bindgen yeterli)

## Hypothesis
**C++ header'larından ve küçük bir sahiplik açıklama dosyasından otomatik olarak güvenli Rust API'si üretmenin**, **Cargo kullanan ve elinde C++ kütüphanesi olan Rust geliştiricilerini** **elle bridge ve `unsafe` yazmaktan kurtaracağına** inanıyoruz.
**Haklı olduğumuzu şundan anlayacağız:** ArduPilot `AP_Math` kütüphanesinin public API'sinin **en az %80'i**, elle yazılmış hiç bridge olmadan ve kullanıcı hiç `unsafe` yazmadan Rust'tan çağrılabilecek.

## Success Metrics
| Ölçüt | Hedef | Nasıl ölçülür |
|---|---|---|
| `AP_Math` public API kapsamı | ≥ %80 | Desteklenen sembol / toplam public sembol (aracın kendi raporu + elle kontrol) |
| Kullanıcı kodunda `unsafe` | 0 | Demo crate'inde `unsafe` taraması |
| Sessizce atlanan yapı | 0 (%100'ü açıklamalı uyarıyla bildirilir) | Desteklenmeyen yapılar için negatif testler |
| Gereken açıklama satırı | TBD — ilk prototipte ölçülecek (100 API öğesi başına satır) | `ferrobridge.toml` satır sayısı / üretilen öğe sayısı |
| v0.1 sonrası dış kullanım | TBD — yayın sonrası belirlenecek | crates.io indirme sayısı, GitHub issue/yıldız, geri bildirim |

## Scope
**MVP:**
- Yön: Rust → C++ (C++'ı Rust'tan çağırmak)
- Desteklenen yapılar: serbest fonksiyonlar, sınıflar ve metotlar, enum'lar
- Desteklenen standart tipler: `std::string`, `std::unique_ptr` (Milestone 2); `std::vector` (Milestone 3, 27.11.2026)
- C++ exception'ları Rust'ta `Result` olarak dönecek
- Sahiplik bilgisi kullanıcının yazdığı bir açıklama dosyasından alınacak
- Cargo build entegrasyonu
- Desteklenmeyen her yapı için açıklamalı uyarı
- `AP_Math` üzerinde uçtan uca demo

**Out of scope:**
- Bazel desteği — Crubit zaten çözüyor
- C++ → Rust yönü — Zngur'a bırakıldı, ileride birlikte çalışma hedefleniyor
- Genel template, kalıtım/virtual, operator overloading, callback/lambda — v0.2 ve sonrasına ertelendi
- CMake entegrasyonu — v0.2'nin ilk hedefi (PX4 ile test)
- Yalnızca nightly Rust'ta çalışan özellikler — MVP stable Rust ile çalışmalı
- Saf C kütüphaneleri — bindgen yeterli
- Açıklanmamış ham pointer'lar için sahiplik tahmini — araç tahmin yürütmez, kullanıcıya sorar

## Delivery Milestones
| # | Milestone | Outcome | Status | Plan |
|---|---|---|---|---|
| 1 | Header'ı okuma | Kullanıcı bir C++ header'ını verir, araç içinde ne gördüğünü (fonksiyonlar, tipler) okunabilir biçimde gösterir | complete | `.claude/plans/m1-header-okuma.plan.md` |
| 2 | İlk köprü | Basit bir C++ sınıfı, elle bridge yazılmadan Rust testinden çağrılabilir | complete | `.claude/plans/m2-ilk-kopru.plan.md` |
| 3 | Sahiplik açıklamaları | Kullanıcı belirsiz pointer'lar için açıklama yazar; araç belirsiz durumda kod üretmek yerine soru sorar | complete | `.claude/plans/m3-sahiplik-aciklamalari.plan.md` |
| 4 | Güvenli API | Kullanıcı hiç `unsafe` yazmadan C++ nesnesi oluşturup kullanabilir; exception'lar `Result` olarak gelir | pending | — |
| 5 | Gerçek dünya demosu | `AP_Math` API'sinin ≥ %80'i Rust'tan kullanılabilir | pending | — |
| 6 | v0.1 yayını | Araç crates.io'da; README, örnekler ve demo herkese açık | pending | — |

## Open Questions
- [ ] Exception'ı kapalı derlenen kodlar (`-fno-exceptions`, ArduPilot gibi gömülü projelerde yaygın): "exception → `Result`" eşlemesi nasıl davranacak?
- [x] Sahiplik açıklamalarının yeri: **ayrı `ferrobridge.toml` dosyası** (üçüncü taraf header'lar düzenlenemez); header içinde C++ attribute'u v0.2'de ek kaynak olabilir. Karar: 27.11.2026, ADR-0006.
- [x] Minimum libclang sürümü: **16** (`= delete` tespiti için gereken fonksiyon 16 ile geldi; CI 21–23'ü test edecek). Karar: 27.11.2026, Milestone 2 planı.
- [ ] Proje adı kesinleşecek mi? (crates.io'da müsaitlik kontrolü)
- [x] Lisans: **MIT OR Apache-2.0**, proje tamamen açık kaynak (karar 26.11.2026, ADR-0003)

## Risks
| Risk | Olasılık | Etki | Önlem |
|---|---|---|---|
| Sahiplik yanlış eşlenir, UB oluşur | Orta | Kritik | Belirsizse kod üretme, kullanıcıya sor; varsayılan en kısıtlayıcı eşleme |
| Kapsam büyür, MVP gecikir | Yüksek | Yüksek | Kapsam dışı listesi; MVP dışı her özellik için karar kaydı (ADR) şartı |
| Crubit Cargo desteği getirir, fark kapanır | Orta | Yüksek | 3 ayda bir rakip kontrolü; fark olarak CMake, açıklama dosyası ve açıklayıcı tanılara odaklan |
| libclang sürüm farkları uyumluluğu bozar | Yüksek | Orta | Birden çok LLVM sürümüyle test; header okuma katmanını izole et |
| Kullanıcı talebi varsayımı yanlış çıkar | Orta | Yüksek | Doğrulama yöntemlerini (issue taraması, forumlar, demo geri bildirimi) Milestone 5'ten önce uygula |
| Açıklama dosyası yazmak kullanıcıya fazla yük olur | Orta | Orta | Prototipte açıklama satırı sayısını ölç; çıkarım kurallarını güçlendir |

---
*Durum: TASLAK — yalnızca gereksinimler. Uygulama planı `/ecc:plan` ile hazırlanacak.*
