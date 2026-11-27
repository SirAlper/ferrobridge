# ADR-0010: Bit bit kopyalanabilen sınıflar Rust değer tipi olur

**Tarih**: 2026-11-27
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

ADR-0007'ye göre her C++ sınıfı opak bir cxx tipiydi ve Rust'ta `UniquePtr` ya da `Pin` arkasında kullanılıyordu. `Vector3f { float x, y, z; }` gibi küçük, alanları açık matematik tipleri için bu ağır: `v.x`'e erişilemiyor, her vektör yığında ayrılıyor, değiştirmek için `Pin<&mut>` gerekiyor. 3. güvenlik kuralı yalnızca trivial *olmayan* taşınabilir tiplerin değer olarak tutulmasını yasaklıyor.

## Karar

Aşağıdaki koşulların **hepsi** sağlanırsa bir sınıf ya da örnekleme Rust'ta `#[repr(C)] #[derive(Debug, Clone, Copy)] pub struct` olur:
- polimorfik değil, taban sınıfı yok
- kullanıcı tanımlı kopyalama, taşıma ya da destructor yok (`= default` hariç)
- bütün statik olmayan alanları public, `const` değil, bayt hizalı
- alan tipleri sayı, `bool`, `c_char` ya da başka bir değer tipi
- düzeni libclang'dan biliniyor

Koşulları sağlamayan sınıflar opak kalır.

- cxx'e `unsafe impl ::cxx::ExternType` (`Kind = Trivial`) ile tanıtılır. `unsafe` yalnızca üretilen iç katmanda, `// SAFETY:` gerekçesiyle bulunur.
- **Son sözü C++ derleyicisi söyler.** `shim.cc` şunları `static_assert` ile doğrular: `is_trivially_copyable`, `is_standard_layout`, `sizeof`, `alignof` ve her alanın `offsetof`'u. Rust tarafı da `size_of`, `align_of` ve `offset_of!` ile doğrular. cxx de kendi `IsRelocatable` kontrolünü ekler. Analiz yanılırsa sonuç sessiz UB değil, derleme hatasıdır.
- Değerle parametre ve dönüş, `&T` / `&mut T` (`Pin` yok); constructor değer döndürür.
- `PartialEq` ve `Default` türetilmez; C++'taki `==` ve varsayılan constructor anlamını taşımayabilirler. `==` C++ operatöründen bağlanır.
- Üye operatörler hata dönmüyorsa `std::ops` trait'leri olur (`Add`, `Mul<f32>`, `Neg`, `AddAssign`, `PartialEq`...). Her durumda adlı metotlar (`op_add`) da vardır. Karşılaştırmalar adlı metot kalır.
- **`operator[]` hiç bağlanmaz.** C++'ta genelde sınır denetimi yapılmadan kullanılır; güvenli Rust'tan sınır dışı bir indeks UB olurdu.

## Değerlendirilen alternatifler

### Alternatif 1: Opak tip + alanlar için getter/setter shim'leri
- **Artıları**: `unsafe impl` gerekmez.
- **Eksileri**: Her alan erişimi bir FFI çağrısı, her nesne yığında. `Vector3f` gibi tiplerde kullanılamayacak kadar ağır ve idiomatik değil.
- **Neden seçilmedi**: Değer tipleri projenin "idiomatik" hedefinin özü. `static_assert`'ler güvenliği derleyiciye doğrulatıyor.

### Alternatif 2: cxx'in paylaşılan struct'ları (`struct` bridge içinde)
- **Neden seçilmedi**: cxx paylaşılan struct'ın C++ tanımını kendisi üretir. Kütüphanenin var olan sınıfıyla çakışır.

## Sonuçlar

### Olumlu
- `v.x`, `a + b`, `v.normalize()` (`&mut self`). Yığın ayırma ya da `Pin` yok.
- Bir sınıf tasarım gereği değer tipi değilse (private veya `const` alan, sanal fonksiyon) otomatik olarak opak kalıyor.

### Olumsuz
- Bütün alanları public olan bir sınıf Rust'ta alanlarından kurulabiliyor; C++ constructor'ının kurduğu bir değişmez varsa atlanır. Bütün alanları public olan bir C++ sınıfı da aynı şeye izin verdiği için kabul edildi.
- `prettyplease`'in iki sınırı yüzünden üretilen kod biçimlendirmeden sonra metin üzerinde işleniyor: takma adlar ve `SAFETY` yorumları.

### Riskler
- Bir alan tipinin C++ ve Rust gösterimleri aynı ama geçerli değer kümeleri farklı olabilir (ör. `bool`). C++'ta `bool` yalnızca 0/1 olabilir; Rust da aynı şeyi varsayar.
