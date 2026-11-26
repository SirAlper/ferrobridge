# ADR-0003: Açık kaynak, MIT OR Apache-2.0 çift lisansı

**Tarih**: 2026-11-26
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

Proje sahibi ferrobridge'in tamamen açık kaynak olacağını belirledi. PRD'de lisans açık soru olarak duruyordu. Hedef kullanıcılar ferrobridge'i kendi (çoğu zaman ticari) projelerinin build sürecine ekleyecek; bu yüzden lisansın şirketlerin hukuk birimlerinde sorun çıkarmaması gerekiyor. Üzerine inşa ettiğimiz cxx ve bindgen gibi araçlar Rust ekosisteminin standart lisansını kullanıyor.

## Karar

Tüm crate'ler `MIT OR Apache-2.0` çift lisansıyla dağıtılır. Kökte `LICENSE-MIT` ve `LICENSE-APACHE` bulunur; README'de Rust projelerinde yaygın olan katkı maddesi yer alır (katkılar da aynı çift lisansla kabul edilir). crates.io yayını v0.1'e (Milestone 6) kadar `publish = false` ile kapalı kalır.

## Değerlendirilen alternatifler

### Alternatif 1: Yalnızca MIT
- **Artıları**: En kısa ve en basit lisans.
- **Eksileri**: Açık patent lisansı yok.
- **Neden seçilmedi**: Apache-2.0'ın patent maddesi kurumsal kullanıcılar için önemli; çift lisans ikisini de sunuyor.

### Alternatif 2: Yalnızca Apache-2.0
- **Artıları**: Patent koruması.
- **Eksileri**: GPLv2 projeleriyle uyumsuz.
- **Neden seçilmedi**: GPLv2 kod tabanlarıyla birlikte kullanım imkânını kapatmak istemiyoruz.

### Alternatif 3: Copyleft (GPL / MPL-2.0)
- **Artıları**: Türev çalışmaların da açık kalmasını sağlar.
- **Eksileri**: Bir build aracının ürettiği kodun ve kütüphanelerin lisans durumu kullanıcı için belirsizleşir; şirketlerde benimsenmeyi zorlaştırır.
- **Neden seçilmedi**: Hedef, aracın mümkün olduğunca geniş kullanılması.

## Sonuçlar

### Olumlu
- Rust ekosistemiyle (cxx, bindgen, Rust'ın kendisi) aynı lisans; kullanıcılar için sürpriz yok.
- Kurumsal kullanıma ve GPLv2 projelerine açık.

### Olumsuz
- Başkaları kodu kapalı kaynak ürünlerde kullanabilir; değişikliklerini geri vermek zorunda değiller.

### Riskler
- **Üçüncü taraf kod ve fixture'lar (ör. ArduPilot `AP_Math`, GPLv3) repoya kopyalanırsa lisans karışır.** Önlem: GPL lisanslı kodu repoya **kopyalamıyoruz**; demo ve testler bu kodu dışarıdan (ör. git submodule veya indirme betiği) kullanır ve lisansı ayrıca belirtilir. Kendi yazdığımız küçük fixture header'ları çift lisans altındadır.
