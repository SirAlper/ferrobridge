# Milestone 5 öncesi ölçüm: ferrobridge `AP_Math` üzerinde (2026-11-27)

**Amaç:** M5'in başarı ölçütü "`AP_Math` public API'sinin ≥ %80'i Rust'tan kullanılabilir". Plana başlamadan önce bugünkü ferrobridge'in (`40dce02`, M4 sonu) nerede durduğunu ölçmek.

**Kaynak:** ArduPilot `master`, commit `32e4f8e` (2026-11-27). Repo dışına, sığ ve seyrek klonlandı. GPLv3 kodu bu repoya kopyalanmadı (ADR-0003); burada yalnızca sayılar ve sembol adları var.

## Özet
- **Bugün: ~%20.** Kabaca 424 public API öğesinden 86'sı bağlanıyor. Bağlananlar `AP_Math.h`, `control.h`, `crc.h`, `location.h` gibi header'lardaki `float`/`int` alan serbest fonksiyonlar ve dört küçük sınıf (`SCurve`, `SplineCurve`, `chirp`, `AP_GeodesicGrid`).
- **Ana engel template'ler.** `Vector2/3`, `Matrix3` ve `QuaternionT` sınıf template'leri kütüphanenin çekirdeği. İçlerinde ~228 public üye var. Üstüne 27 serbest fonksiyon `Vector3f` gibi tipler aldığı için, 23 fonksiyon da kendisi template olduğu için atlanıyor.
- **%80'e ulaşmak için bugünkü kapsamın dışındaki birkaç yetenek birlikte gerekiyor.** Kaba tahmin:
  - Sınıf template örnekleme ve public alanlar: ~%74.
  - Buna fonksiyon template örnekleme ve pointer + uzunluk desteği eklenirse: ~%79–80.
  - Operatörlerle birlikte: ~%95.
- **Ölçüm, araçta düzeltilmesi gereken iki sınır da buldu** (aşağıda "Bulgular").

## Yöntem
- Her `libraries/AP_Math/*.h` için `ferrobridge generate` ve `ferrobridge parse` çalıştırıldı.
- clang argümanları: `-std=c++17`, `-I libraries`, SITL define'ları (`CONFIG_HAL_BOARD=HAL_BOARD_SITL`, `CONFIG_HAL_BOARD_SUBTYPE=HAL_BOARD_SUBTYPE_NONE`, `HAL_PROGRAM_SIZE_LIMIT_KB=4096`) ve boş bir `hwdef.h` stub'ı. waf bu dosyayı derleme sırasında üretir.
- Kendi başına derlenmeyen dört header (`SCurve`, `SplineCurve`, `div1000`, `rotations`) `-include stdint.h -include AP_Math/AP_Math.h` ile önden yüklendi. ArduPilot'ta bunlar hep `AP_Math.h`'den sonra include ediliyor.
- Template üyeleri ferrobridge tarafından okunmadığı için (template'in tamamı tek bir "atlandı" öğesi oluyor) sınıf gövdeleri ayrı bir betikle **kabaca** sayıldı: yalnızca public metot, operatör, constructor ve alan. Bu sayılar ±%10 hatalı olabilir.

## Sayılar
| | Öğe |
|---|---|
| Bağlanan fonksiyon/metot/constructor | **86** |
| Atlanan (template sınıfların kendisi hariç) | 109 |
| Template sınıfların public üyeleri (kaba) | ~228 (`Vector3` 56, `Vector2` 49, `Matrix3` 46, `QuaternionT` 52, `VectorN` 17, `MatrixN` 6, `PolyFit` 2) |
| Ölçülemeyen (`rotations.h`: `Rotation` enum'u) | 1 |
| **Toplam (payda)** | **~424** |

Atlananların nedenleri (template sınıflar hariç):
| Neden | Öğe | Not |
|---|---|---|
| Parametre/dönüş tipi `Vector2f`, `Vector3f`... (template) | 27 | Sınıf template örneklemesiyle çözülür |
| Kendisi template olan fonksiyon (`is_zero<T>`, `constrain_value<T>`, `wrap_180`...) | 23 | Fonksiyon template örneklemesi gerekir |
| Pointer + uzunluk (`crc_*(const uint8_t* buf, uint16_t len)`, `fill_nanf`) | 25 | **`borrowed` ile açıklanamaz:** tek nesne değil, buffer. Slice desteği gerekir (M3'te v0.2'ye ertelendi) |
| Const olmayan primitif `T&` (`control.h` çıkış parametreleri) | 8 | **Bugün `ferrobridge.toml` ile bağlanabilir** |
| `static inline` fonksiyonlar (`radians`, `degrees`, `sq`, `cd_to_rad`...) | 8 | Bkz. bulgu 3 |
| Global/statik sabitler (`WGS84_A`, `NaNf`...) | 9 | Rust `const` olarak üretilebilir (libclang sabit değerini hesaplayabiliyor) |
| Tanınmayan bildirim (template özelleştirmesi `Vector3<float>::is_zero` ×4, `AP_PARAMDEFV` makrosu ×1) | 5 | |
| Diğer (`float[5]` dizi parametresi, `void*`, `size_t`, başka header'daki `Rotation`) | 4 | |

## Kümülatif senaryolar (kaba)
| Eklenen yetenek | Kazanılan | Toplam bağlanan | Kapsam |
|---|---|---|---|
| Bugün | | 86 | %20 |
| + `control.h` çıkış parametreleri için `ferrobridge.toml` (yazmak yeterli) | +8 | 94 | %22 |
| + `static inline` header fonksiyonlarına izin | +8 | 102 | %24 |
| + Pointer + uzunluk → `&[T]` | +25 | 127 | %30 |
| + Sınıf template örnekleme (`Vector2f/3f`, `Matrix3f`, `Quaternion`) + public alanlar (operatörler hariç) | +186 | 313 | %74 |
| + Fonksiyon template örnekleme | +23 | 336 | %79 |
| + Sabitler (`const`) | +9 | 345 | %81 |
| + Operatörler (`Add`, `Mul`, `Index`...) | +69 | 414 | %98 |

## Bulgular (araçta düzeltilmesi gerekenler)
1. **Kendini dolaylı olarak include eden header okunamıyor.** `matrixN.h`, `vectorN.h` ve `rotations.h`, `AP_Math.h` üzerinden kendilerini yeniden include ediyor. ferrobridge header'ı libclang'a "ana dosya" olarak veriyor; ana dosyada `#pragma once` koruma yapmıyor ve sınıf iki kez tanımlanıyor (`redefinition of 'VectorN'`). Çözüm: `#include "<header>"` içeren sentetik bir çeviri birimi parse edip yalnızca o header'daki bildirimleri almak.
2. **Kendi başına derlenmeyen header'lar** (`SCurve.h` `Vector3f`'i include etmeden kullanıyor, `rotations.h` `uint8_t`'yi). Bugün `-- -include ...` ile aşılabiliyor; bunun belgelenmesi ya da `ferrobridge.toml`'da bir `prelude` ayarı gerekiyor.
3. **`static inline` fonksiyonlar gereğinden katı reddediliyor olabilir.** Kural, "iç bağlantılı fonksiyonlar header dışından çağrılamaz" diyor. Ama cxx'in ve shim'in ürettiği C++ dosyası header'ı include ediyor; `static inline` fonksiyon o çeviri biriminde görünür. Hipotez: güvenle bağlanabilirler. Doğrulanmalı (anonim namespace'teki tipler için kural ayrıca düşünülmeli).
4. **`AP_Math.h` ArduPilot'un büyük kısmını çekiyor.** `AP_Param` (`AP_PARAMDEFV` makrosu) üzerinden 22 ek kütüphane include ediliyor (toplam 28). Ayrıca `hwdef.h` waf tarafından üretiliyor. Parse için stub yetti. Ama demo crate'inin `.cpp` dosyalarını **derleyip bağlaması** için `AP_HAL` ve `AP_InternalError` gibi bağımlılıkların stub'lanması gerekebilir. Bu, M5'in en büyük derleme riski.
5. **`-fno-exceptions`:** Demo crate'i `AP_Math`'i kendisi derleyeceği için exception'lar açık derlenebilir; `AP_Math` zaten exception fırlatmıyor. Soru asıl, var olan bir `-fno-exceptions` derlemesine gömülen kullanıcılar için açık kalıyor. Demo için engel değil.

## M5 kapsamı için seçenekler
1. **Hedefi koru (≥ %80).** M5'e sınıf ve fonksiyon template örnekleme (PRD'deki v0.2 hedefi: `instantiate = ["Vector3<float>"]`), public alanlar, pointer + uzunluk → slice ve bulgu 1 girer. Kapsam çok büyük: muhtemelen iki milestone'a bölünmeli.
2. **Hedefi ara adıma böl.** M5 = `ferrobridge-build` + demo crate + bulgu 1–3 + sabitler + slice; ölçüm ~%30–35. Template ve alanlar M6'ya, v0.1 yayını M7'ye kayar. PRD'de milestone tablosu değişir.
3. **Hedefi yeniden tanımla.** Ölçütü "template olmayan API'nin ≥ %80'i" yapmak. Bugün ~%59 (86/145); slice, `static inline` ve çıkış parametreleriyle ~%88. Ama `Vector3f` olmadan `AP_Math` demosu ikna edici olmaz.

**Öneri:** Seçenek 2. Template örnekleme, ferrobridge'in M1'den beri ertelenmiş en büyük tasarım işi ve kendi milestone'unu hak ediyor. M5 önce derleme hattını ve demo iskeletini gerçek bir kütüphane üzerinde ayağa kaldırır. Bu en riskli kısım (bulgu 4); template işine de sağlam bir zemin hazırlar.

## Tekrar üretme
```bash
git clone --depth 1 --filter=blob:none --sparse https://github.com/ArduPilot/ardupilot.git
cd ardupilot && git sparse-checkout set libraries/AP_Math libraries/AP_Common libraries/AP_HAL libraries/AP_HAL_SITL libraries/AP_Param ...
# (eksik kütüphaneler parse hatalarından tek tek eklendi; toplam 28 kütüphane)
mkdir -p ../stubs && : > ../stubs/hwdef.h
ferrobridge generate libraries/AP_Math/vector3.h --out-dir /tmp/out -- \
  -I libraries -I ../stubs -DCONFIG_HAL_BOARD=HAL_BOARD_SITL \
  -DCONFIG_HAL_BOARD_SUBTYPE=HAL_BOARD_SUBTYPE_NONE -DHAL_PROGRAM_SIZE_LIMIT_KB=4096
```

## M5 sonrası (2026-11-27)

Aynı yöntem; ArduPilot `437a3301cc` (ölçümdekinden yeni bir commit), bütün `AP_Math/*.h` header'ları `--prelude AP_Math/AP_Math.h` ile. Bağlanan öğe = fonksiyon/metot/constructor + sabit + enum. Template sınıf üyeleri yine kaba sayımla ~228.

| | Bağlanan | Template olmayan API | Toplam |
|---|---|---|---|
| M4 (ölçüm) | 86 | 86/145 = %59 | 86/424 = %20 |
| M5, açıklamasız | 99 | 99/146 = %68 | 99/424 = %23 |
| **M5, demonun açıklamalarıyla** (`demo/ap_math/annotations/`) | **131** | **131/146 = %90** | **131/424 = %31** |

Hedefler (template olmayan ≥ %85, toplam ≥ %30) tuttu. Bulgu 1–3 kapandı; bütün header'lar okunuyor (`vectorN.h`, `matrixN.h`, `rotations.h` dahil). `Rotation` enum'u da bağlanıyor.

Template olmayan API'de kalan 15 öğe:
- **Bilinçli açıklanmayanlar:** `crc_crc4`'te uzunluk parametresi yok; `crc8_dvb_s2_update` `void*` alıyor.
- **Platforma bağlı `long`:** `wrap_360_cd(long)`.
- **Başka header'daki bir tip:** `rotation_equal(Rotation, Rotation)`. `Rotation` `rotations.h`'ta; header'lar arası tipler henüz bağlanmıyor.
- **Değeri fonksiyon çağrısıyla hesaplanan sabitler (4):** `NaNf`, `DEG_TO_RAD_DOUBLE`, `RAD_TO_DEG_DOUBLE`, `WGS84_E`.
- **Tanınmayan bildirimler:** `AP_PARAMDEFV` makrosu ve template özelleştirmeleri.

Kaynakları okurken çıkan bulgu: `crc8_dvb_update`, adına rağmen DVB-S2'nin polinomunu (0xD5) değil 0x07'yi kullanıyor; yani CRC-8/SMBUS. Demo testi bunu `crc8_dvb_s2` ile karşılaştırarak gösteriyor.
