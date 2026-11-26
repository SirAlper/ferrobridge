# ADR-0006: Sahiplik açıklamaları ayrı bir TOML dosyasında

**Tarih**: 2026-11-27
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

C++ header'ı bir pointer'ın sahibini ve ömrünü söylemez. ferrobridge bunu tahmin etmez (güvenlik kuralı 1); bilgi kullanıcıdan gelmelidir. Bağlanacak kütüphanelerin çoğu üçüncü tarafındır (ilk hedef ArduPilot `AP_Math`, GPLv3; kodu repoya kopyalanmaz ve header'ları değiştirilemez). Açıklama yanlışsa üretilen kod tanımsız davranışa yol açabilir; bu yüzden açıklamaların tek bir yerde, gözden geçirilebilir biçimde durması ve yazım hatalarının sessizce yok sayılmaması gerekir. PRD'deki "açıklamaların yeri" açık sorusu bu kararla kapanır.

## Karar

Sahiplik açıklamaları header'ın yanında duran ayrı bir `ferrobridge.toml` dosyasında yazılır ve `ferrobridge generate --config` ile verilir. Anahtar tam C++ adıdır (`ns::Sınıf::metot`, constructor için `ns::Sınıf::Sınıf`); değerler `returns = "owned" | "borrowed"` ve `params.<ad> = "borrowed"`'dır. Her açıklama kullanıcının verdiği bir **sözdür**: kullanılan her söz üretilen dosyaların başında "Açıklamaya dayanan varsayımlar" olarak listelenir; header'da karşılığı olmayan açıklama hata, gereksiz açıklama uyarıdır. `ferrobridge suggest` açıklama bekleyen yerler için yorum satırlı bir taslak basar; değerleri kullanıcı bilerek açar.

## Değerlendirilen alternatifler

### Alternatif 1: Header içinde C++ attribute'u (`[[clang::annotate("fb::owned")]]`)
- **Artıları**: Bilgi bildirimin hemen yanında; header değişince birlikte değişir.
- **Eksileri**: Üçüncü taraf header'ları düzenlenemez; kendi kodunda bile her derleyicide bilinmeyen attribute uyarısı.
- **Neden seçilmedi**: İlk hedef kullanıcı senaryosunda (ArduPilot) kullanılamaz. Kendi kodunu bağlayanlar için v0.2'de ek bir kaynak olarak eklenebilir; ayrı dosya yine geçerli kalır.

### Alternatif 2: Açıklamaları `Cargo.toml` `[package.metadata.ferrobridge]` altına yazmak
- **Artıları**: Ek dosya yok.
- **Eksileri**: CLI kullanımı Cargo'ya bağlanır; birden fazla header için tek dosyada karışır; satır numaralı hata vermek zorlaşır.
- **Neden seçilmedi**: Açıklamalar bir header'ın sözleşmesidir, paketin değil. `build.rs` entegrasyonu (`ferrobridge-build`) aynı TOML dosyasının yolunu alabilir.

### Alternatif 3: `suggest` taslağındaki değerleri dolu basmak (ör. ilk seçenek)
- **Artıları**: Kullanıcı daha az yazar.
- **Eksileri**: Taslağı olduğu gibi kabul etmek sahipliği tahmin etmek demektir; yanlış seçenek UB'dir.
- **Neden seçilmedi**: Güvenlik kuralı 1. Değerler yorum satırıdır; açılmayan satırın öğesi bağlanmamaya devam eder.

## Sonuçlar

### Olumlu
- Üçüncü taraf kütüphaneler header'a dokunmadan bağlanabilir.
- Güven noktaları tek dosyada ve üretilen her dosyanın başında; kod incelemesinde tek bakışta görülür.
- Yazım hataları (bilinmeyen anahtar/değer, olmayan fonksiyon/parametre) satır numarasıyla hata verir.

### Olumsuz
- Header değişince (ör. parametre adı) açıklama dosyası da güncellenmeli; bilinmeyen ad hatası bunu yakalar.
- Anahtar aynı adlı **tüm overload'lara** uygulanır; overload'ları ayrı açıklamak (imzalı anahtar) v0.2'ye kaldı.

### Riskler
- Kullanıcı yanlış söz verir (ör. `owned` dediği nesne `new` ile ayrılmamış) → UB. Önlem: varsayımlar listesi, README'de net uyarı, ucuz kontroller her zaman yapılır (ödünç dönüşte null → `Err`; sahip olunan dönüşte destructor public mi, polimorfik sınıfta sanal mı).
- Bir açıklama birden fazla overload'a uygulanır ve biri için yanlıştır. Önlem: bu durumda uyarı, uygulanan overload'ların imzalarıyla basılır.
