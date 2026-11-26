# ADR-0001: C++ header'larını okumak için libclang'ı `clang-sys` üzerinden kullanmak

**Tarih**: 2026-11-26
**Durum**: accepted
**Karar verenler**: Alper (proje sahibi)

## Bağlam

ferrobridge'in C++ header'larından fonksiyon, sınıf, metot, enum, tip boyutu ve exception bilgisini doğru okuması gerekiyor. Makrolar, `#include`'lar ve derleme bayrakları (`compile_commands.json`) da hesaba katılmalı. Parser birden çok LLVM sürümünde çalışmalı ve kullanıcının kurulumu zor olmamalı: tek bir `brew install llvm` (ya da dağıtımın paket yöneticisi) yeterli olmalı. Geliştirme makinesinde 26.11.2026 itibarıyla Homebrew LLVM 23.1.6 kurulu.

## Karar

libclang'ın C API'sini **`clang-sys`** crate'i (1.9.1, Temmuz 2026'da güncellendi) üzerinden kullanıyoruz. Bunun üstüne `ferrobridge-parser` içinde kendi ince güvenli sarmalayıcımızı yazıyoruz. Bu, bindgen'in de kullandığı kanıtlanmış yöntem. Parser çıktısı yalnızca IR'a dönüşüyor; IR libclang'dan habersiz.

## Değerlendirilen alternatifler

### Alternatif 1: `clang` crate (aynı yazarın üst seviye sarmalayıcısı, 2.1.0)
- **Artıları**: Hazır güvenli API, hızlı başlangıç.
- **Eksileri**: libclang'ın yeni fonksiyonlarına geç kavuşur. Eksik bir fonksiyon için alt katmana inmek zorlaşır.
- **Neden seçilmedi**: Hangi libclang fonksiyonunu nasıl kullandığımızı kendimiz kontrol etmek istiyoruz. İleride yeniden değerlendirilebilir.

### Alternatif 2: Clang LibTooling (C++ yardımcı program)
- **Artıları**: AST'ye tam erişim: template'ler, triviality, attribute'lar.
- **Eksileri**: C++ API'si LLVM sürümleri arasında kararsız. Kullanıcının LLVM geliştirme kütüphanelerine karşı derleme yapması gerekir. Projede ikinci bir dil ve build zinciri oluşur.
- **Neden seçilmedi**: MVP için fazla maliyetli. Kaçış yolu olarak açık tutuluyor (bkz. Riskler).

### Alternatif 3: `clang -Xclang -ast-dump=json`
- **Artıları**: Kod yazmadan tam AST alınır.
- **Eksileri**: JSON biçimi kararlı değil, çıktı çok büyük, her header için ayrı süreç gerekir.
- **Neden seçilmedi**: Ana parser olarak uygun değil. Hata ayıklama ve karşılaştırma aracı olarak kullanılabilir.

### Alternatif 4: bindgen'i kütüphane olarak kullanmak
- **Artıları**: Olgun libclang kodu.
- **Eksileri**: İç IR'ı public ve kararlı bir API değil. autocxx bu yüzden bindgen'i fork etmek (`autocxx-bindgen`) zorunda kaldı ve bakım yükü taşıyor.
- **Neden seçilmedi**: autocxx'in yaşadığı bakım yükünü tekrarlamamak için.

### Alternatif 5: tree-sitter-cpp
- **Artıları**: Hızlı, LLVM gerektirmez.
- **Eksileri**: Tip çözümlemesi, makro açma ve `#include` takibi yok.
- **Neden seçilmedi**: Anlamsal bilgi olmadan güvenli kod üretilemez.

## Sonuçlar

### Olumlu
- Kurulum basit: `brew install llvm` ve `LIBCLANG_PATH` ayarı yeterli.
- libclang C API'si sürümler arasında kararlı tutuluyor.
- bindgen'in libclang kullanım kodu (`clang.rs`) öğrenme kaynağı olarak hazır.

### Olumsuz
- libclang tüm C++ AST'sini sunmuyor. Bilinen boşluklar: sınıf template'lerinin alt öğeleri (metot, constructor) görünmüyor, bazı anlamsal sorgular eksik. C2Rust ekibi bu yüzden LibTooling'e geçti.
- Kendi sarmalayıcımızın bakımını biz yapacağız.

### Riskler
- **libclang bir MVP özelliği için yetersiz kalabilir** (ör. bir tipin güvenle taşınabilir olup olmadığını anlamak). Önlem: Milestone 2–4'te bir spike ile erken test edilir. Yetersiz kalırsa yalnızca o bilgiyi sağlayan küçük bir LibTooling yardımcısı eklenir; bunun için yeni bir ADR yazılır.
- **LLVM sürüm farkları.** Önlem: CI matrisi güncel sürümleri kapsar (ör. LLVM 21–23). Kesin minimum sürüm PRD'deki açık soruda belirlenecek.

## Kaynaklar
- [Status/Future of libclang (LLVM Discourse)](https://discourse.llvm.org/t/status-future-of-libclang/41709)
- [libclang API is not sufficient for tools like bindgen (LLVM bug 54236)](https://lists.llvm.org/pipermail/llvm-bugs/2022-March/098440.html)
- [libclang or libtooling for transpiler (cfe-dev)](https://lists.llvm.org/pipermail/cfe-dev/2019-April/062179.html)
- [Clang: Choosing the Right Interface](https://rocm.docs.amd.com/projects/llvm-project/en/docs-6.4.1/LLVM/clang/html/Tooling.html)
- [bindgen clang.rs](https://docs.rs/bindgen/0.72.1/src/bindgen/clang.rs.html)
