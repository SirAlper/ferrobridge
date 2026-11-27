# ADR-0009: Çok header'lı bridge'ler ve template örnekleme

**Tarih**: 2026-11-27
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

Gerçek C++ kütüphanelerinde tipler birden fazla header'a dağılır. `AP_Math`'te `Vector2f` `vector2.h`'ta tanımlı; onu kullanan fonksiyonlar `control.h`, `location.h` ve `SCurve.h`'ta. M5'e kadar her header ayrı bağlanıyordu ve başka bir header'daki tipi kullanan her fonksiyon atlanıyordu. Kütüphanenin çekirdek tipleri de (`Vector3<T>`, `Matrix3<T>`, `QuaternionT<T>`) sınıf template'leri. PRD template'leri MVP dışında bırakmıştı, ama M5 ölçümü onlarsız %80 hedefine ulaşılamayacağını gösterdi.

## Karar

1. **Bir bridge birden fazla header'dan üretilir.** CLI: `ferrobridge generate a.h b.h ...`; build: `Generate::new("ad").header(..)`.
   - Parser hepsini tek bir sentetik çeviri biriminde okur. Analiz tipleri bütün header'lar arasında tanır.
   - Bridge'e eklenmemiş bir header'daki tip atlanır, ama mesaj o header'ı adıyla söyler (IR'da `declared_in`).
2. **Sınıf template'leri yalnızca kullanıcının listelediği somut tiplerle örneklenir:**
   ```toml
   [templates]
   Vector3f = "Vector3<float>"
   ```
   - Örneklemenin argümanları ve düzeni libclang'a çözdürülür. Sentetik birime `using` takma adı ve sınıfı örneklemeye zorlayan bir `static_assert(sizeof(...))` eklenir.
   - libclang örneklenmiş bir sınıfın üyelerini göstermediği için üyeler template'in kalıbından okunur. Kalıptaki `type-parameter-0-N` somut argümanla değiştirilir.
   - Bağımlı bir başvuru (`Vector2<T>`), listede o argümanlarla bir örnekleme varsa ona bağlanır. Tanınmayan her bağımlı tip `Unsupported` olur.
   - C++ tarafında shim `using Vector3f = ::Vector3<float>;` takma adını tanımlar. Açık örnekleme **yapmaz**; kütüphanenin kendi `template class ...;`'ıyla çakışırdı.
3. **Kapsam:** Yalnızca tip parametreli sınıf template'leri. Tip olmayan parametreler, üye template'leri ve fonksiyon template'leri gerekçeyle reddedilir.

## Değerlendirilen alternatifler

### Alternatif 1: Ayrı bridge'ler arasında tip paylaşmak
- **Artıları**: Her header kendi modülünde kalır.
- **Eksileri**: cxx'te başka bir bridge'in tipi `ExternType` takma adıyla içeri alınır. Kullanıcı bridge'ler arası bağımlılıkları yönetmek zorunda kalır; döngüsel başvurular (iki header birbirinin tipini kullanıyor) çözülemez.
- **Neden seçilmedi**: Çok header'lı tek bridge aynı sonucu kullanıcıya yük getirmeden veriyor.

### Alternatif 2: Header'da kullanılan bütün örneklemeleri kendiliğinden bulmak
- **Artıları**: Yapılandırma gerekmez.
- **Eksileri**: Header'da geçen her `Vector3<int16_t>`'nin kütüphanede tanımlı üyeleri olmayabilir (yalnızca `.cpp`'de açıkça örneklenenler bağlanabilir); bağlama hatası çıkar. Hangi örneklemenin bağlanacağı tahmin olur.
- **Neden seçilmedi**: 1. güvenlik kuralı (tahmin etme, sor).

### Alternatif 3: libclang'ın örneklenmiş sınıfı (açık örnekleme dahil) üzerinden gezmek
- **Neden seçilmedi**: Mümkün değil. libclang örneklemelerin üyelerini cursor olarak göstermiyor (M6 Task 0'da doğrulandı).

## Sonuçlar

### Olumlu
- `AP_Math`'te `Vector2f/3f`, `Matrix3f`, `Quaternion` ve `double` sürümleri bağlanıyor; başka header'lardaki kullanımları da açılıyor.
- Tek bir `ferrobridge.toml` bütün bridge'e yetiyor.

### Olumsuz
- İkame motoru yeni ve karmaşık bir kod. Yanlış bir ikame derleme hatası olur, sessiz UB olmaz: shim ve cxx tipli C++ imzaları üretir.
- Kullanıcı hangi örneklemelerin kütüphanede tanımlı olduğunu bilmeli. Listelediği bir örneklemenin üyeleri derlenmiyorsa bağlama hatası alır.

### Riskler
- Kalıbın libclang'daki gösterimi sürümler arasında değişebilir (ör. `type-parameter-0-N` yazımı). Önlem: iki libclang sürümünde (Apple 21, Homebrew 23) aynı snapshot'lar.
