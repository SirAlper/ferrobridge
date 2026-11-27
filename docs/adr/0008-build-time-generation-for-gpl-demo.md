# ADR-0008: Derleme sırasında üretim (`ferrobridge-build` `generate` özelliği) ve GPL'li demo

**Tarih**: 2026-11-27
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

ADR-0004'e göre üretilen bridge'ler commit'lenir; bu, FFI kodunun görünür ve incelenebilir olmasını sağlar. M5'in gerçek dünya demosu ArduPilot'un `AP_Math` kütüphanesini bağlıyor. ArduPilot GPL-3.0 lisanslı. ADR-0003 gereği GPL kodu bu depoya kopyalanmıyor. ArduPilot header'larından üretilen `bridge.rs` ve `shim.*` ise ArduPilot'un bildirimlerinden, adlarından ve belgelerinden türüyor. Bunları commit'lemek, depoya GPL'den türemiş metin sokmak olur. Ayrıca demo kendi Rust kodunda GPL kütüphanesine bağlanıyor.

## Karar

1. **`ferrobridge-build` iki mod sunar.**
   - Varsayılan mod commit'li dosyaları derler. Bu ADR-0004'ün yolu; libclang gerekmez.
   - `generate` özelliğiyle açılan mod, header'ı `build.rs` içinde libclang ile okur. Bridge'i `$OUT_DIR/ferrobridge/<ad>/` altına üretip derler. Rust tarafı `ferrobridge::include_bridge!("<ad>")` ile kullanır.
2. **Demo `demo/ap_math/` altında, ayrı bir Cargo workspace'idir.**
   - Lisansı `GPL-3.0-or-later`, kendi `LICENSE` dosyasıyla; `publish = false`.
   - ArduPilot'a `ARDUPILOT_DIR` ile dışarıdan işaret eder.
   - `generate` modunu kullanır. Üretilmiş hiçbir dosya commit'lenmez.
3. **ferrobridge'in kendi crate'leri `MIT OR Apache-2.0` kalır.** Kök workspace demoyu içermez; `cargo test --workspace` ArduPilot istemez.

## Değerlendirilen alternatifler

### Alternatif 1: Demoyu da ADR-0004'e göre commit'li dosyalarla kurmak
- **Artıları**: Tek yol; demodaki bridge de incelenebilir.
- **Eksileri**: Depoya GPL'den türemiş üretilmiş metin girer; ArduPilot güncellenince dosyalar elle yeniden üretilmeli.
- **Neden seçilmedi**: Lisans ayrımı (ADR-0003) daha önemli.

### Alternatif 2: Demoyu ayrı bir depoya koymak
- **Artıları**: Lisans ayrımı tam; ana depoda GPL dosyası hiç yok.
- **Eksileri**: Demo ile üretici aynı commit'te değişemez; demonun kırıldığı geç fark edilir.
- **Neden seçilmedi**: M5–M6 boyunca demo, üreticinin ana gerçek dünya testi. Ayrı workspace ve ayrı lisans dosyası yeterli ayrımı sağlıyor. Yayın (M7) öncesinde yeniden değerlendirilebilir.

### Alternatif 3: Derleme sırasında üretimi hiç sunmamak
- **Artıları**: ADR-0004 tek kural olarak kalır.
- **Eksileri**: Lisansı ya da boyutu yüzünden üretilmiş dosyaları commit'leyemeyen kullanıcılar için yol yok.
- **Neden seçilmedi**: Gerçek bir ihtiyaç; açık bir özellik (`generate`) olarak sunmak varsayılanı değiştirmiyor.

## Sonuçlar

### Olumlu
- Depoda GPL'den türemiş üretilmiş dosya yok. `demo/ap_math/.gitignore` `target/`'ı dışarıda bırakıyor.
- `ferrobridge-build` hem commit'li hem derleme sırasında üretim yolunu destekliyor. `ferrobridge-e2e` commit'li yolu, bir entegrasyon testi (`crates/ferrobridge-build/tests/generate_mode.rs`) üretim yolunu sınıyor.

### Olumsuz
- `generate` modunda üretilen kod depoda görünmüyor. Dosyalar `OUT_DIR`'de duruyor; bağlanmayan öğeler ve varsayımlar dosyanın başında listeleniyor. `build.rs` bridge başına bir özet uyarısında dosyanın yolunu basıyor.
- `generate` modu her derlemede libclang istiyor. Libclang sürümü değişince bridge değişebilir; bu, ADR-0004'ün kaçındığı durum ve bu mod için bilinçli olarak kabul ediliyor.

### Riskler
- **Üretilmiş bir dosyanın yanlışlıkla commit'lenmesi.** Önlem: demo yalnızca `OUT_DIR`'e üretiyor ve `target/` yok sayılıyor.
- **Demonun kendi kodu GPL.** Ana crate'lere demo kodu taşınmamalı.
- **Üretim makinesiyle derleme makinesinin farklı olması (commit'li modda).** Bu M5'te ayrıca ele alındı:
  - `sizeof`/`alignof`/`offsetof` içeren sabitler reddediliyor.
  - Tamsayı sabitler `shim.cc`'de `static_assert` ile derleme anında yeniden doğrulanıyor.
