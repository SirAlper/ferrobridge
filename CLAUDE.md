# ferrobridge

C++ kütüphanelerini Rust'tan **güvenli ve idiomatik** şekilde kullanmayı sağlayan bağlama (binding) üreteci.
libclang ile header okur → ara model (IR) → ownership çıkarımı + `ferrobridge.toml` açıklamaları → `cxx` bridge + C++ shim + güvenli Rust API üretir.

Açık kaynak: `MIT OR Apache-2.0` (bkz. `docs/adr/0003-open-source-dual-license.md`).

Durum: **Milestone 5 (derleme hattı ve demo iskeleti) tamamlandı**: `ferrobridge-build` `build.rs`'ten commit'li bridge'leri ya da (`generate` özelliği) derleme sırasında `OUT_DIR`'e üretilenleri derliyor (`ferrobridge::include_bridge!`); `{ slice = "len" }` açıklaması pointer + uzunluğu `&[T]`/`&mut [T]` yapıyor; `static inline` fonksiyonlar ve `const`/`constexpr` sabitler bağlanıyor; header'lar sentetik bir çeviri birimiyle okunuyor (`--prelude`). `demo/ap_math/` ArduPilot `AP_Math`'i gerçek bir checkout'a karşı derleyip Rust'tan çağırıyor (template olmayan API'nin %90'ı, toplamın %31'i). Sırada Milestone 6: template örnekleme ve public alanlar (`/ecc:plan .claude/prds/ferrobridge.prd.md`).

## Belgeler
- Gereksinimler (PRD): `.claude/prds/ferrobridge.prd.md`. Planlama komutu `/ecc:plan .claude/prds/ferrobridge.prd.md` bu dosyayı okur.
- Mimari kararlar: `docs/adr/` (dizin: `docs/adr/README.md`, şablon: `docs/adr/template.md`). Kabul edilmiş ADR'ler: 0001 (libclang via `clang-sys`), 0002 (cxx üstüne inşa), 0003 (açık kaynak çift lisans), 0004 (üretilen kod commit'lenir), 0005 (overload/çakışma adlandırma), 0006 (sahiplik açıklamaları ayrı TOML dosyasında), 0007 (cxx üstünde güvenli API katmanı), 0008 (derleme sırasında üretim ve GPL'li demo).
- Milestone planları: `.claude/plans/` (her planın sonunda uygulama notları ve sapmalar).
- Kişisel notlar ve yerel yollar repoya girmez; proje sahibi bunları Git'e girmeyen `CLAUDE.local.md` dosyasında tutar.

## Çalışma kuralları
- Her milestone: `/ecc:plan` ile plan → onay → TDD → review → commit.
- MVP kapsamının dışına çıkan her karar için ADR yaz (`docs/adr/`).
- GPL lisanslı kodu (ör. ArduPilot) repoya **kopyalama**; dışarıdan kullan (ADR-0003).

## Güvenlik kuralları (değişmez)
1. **Tahmin etme, sor:** Sahipliği belirsiz pointer için kod üretilmez. Açıklamalı uyarı verilir ve `ferrobridge.toml` önerisi basılır.
2. **En kısıtlayıcı varsayılan:** Şüphede `&mut` yerine `Pin<&mut T>` ve `&T` kullanılır.
3. **Trivial olmayan taşınabilir tipler** Rust'ta asla değer olarak tutulmaz: `UniquePtr` veya `Pin` kullanılır.
4. **Exception'lar FFI sınırını geçmez:** Her shim `try/catch` ile sarılır.
5. **Kullanıcıya `unsafe` sızmaz:** `unsafe` yalnızca üretilen iç katmanda, gerekçe yorumuyla bulunur.

## Komutlar
```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"   # macOS'ta ayarlanmazsa Apple'ın libclang'ı yüklenir
cargo run -p ferrobridge-cli -- doctor              # hangi libclang'ın yüklendiğini gösterir
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                              # e2e için C++17 derleyicisi gerekir
ARDUPILOT_DIR=~/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml   # AP_Math demosu
```

Üretici veya fixture'lar değişince `crates/ferrobridge-e2e/src/generated/` yeniden üretilmeli (komut: `crates/ferrobridge-e2e/README.md`; `ferrobridge.toml`'u olan fixture'lar `--config` ile); aksi halde `generated_files_match_the_committed_e2e_files` testi başarısız olur. Üretilen `bridge.rs`'teki her üst düzey öğe `#[rustfmt::skip]` taşır, `cargo fmt` ona dokunmaz. E2e'de güvenli API testleri cxx tipi kullanmaz (`tests/idiomatic.rs` denetler); ham katman testleri `tests/raw_layer.rs`'te. Snapshot değişikliklerini `cargo insta review` ile **elle inceleyip** kabul et.

`demo/ap_math/` kök workspace'in parçası değil ve **GPL-3.0-or-later** (ADR-0008): ArduPilot kodu ve ondan üretilen dosyalar repoya girmez (bridge'ler `OUT_DIR`'e üretilir), demo kodu ana crate'lere taşınmaz. Açıklamalar header başına ayrı dosyada (`demo/ap_math/annotations/<bridge>.toml`), çünkü açıklanan her ad o header'da bulunmalı. Bir açıklama yazmadan önce C++ kaynağını oku: söz kaynağa dayanmalı.

## Mimari (kısa)
`parser` (libclang → IR; header bellekte üretilen bir çeviri birimiyle okunur) → `analysis` (`ferrobridge.toml` + neyin güvenle bağlanacağı + Rust adları → `BridgePlan`) → `codegen` (`bridge.rs` = güvenli katman + ham `ffi`, `shim.h`, `shim.cc`) → `cli` / `build` (`build.rs`). Üretilen kod çalışma zamanında `ferrobridge` crate'ini (`Error`, dönüşümler) kullanır. Bütün `unsafe` `crates/ferrobridge-parser/src/clang.rs`'de.

## Kod kuralları (workspace lint'leriyle zorlanır)
- Testler dışında `unwrap()` / `expect()` yasak (`clippy.toml` testlerde izin verir).
- Her `unsafe` bloğun üstünde `// SAFETY:` gerekçesi zorunlu. Yeni libclang çağrıları yalnızca `clang.rs`'e eklenir.
- Integration test dosyalarında `#![allow(clippy::unwrap_used)]` gerekir (`clippy.toml` istisnası yardımcı fonksiyonları kapsamıyor).
- Public öğelerde doküman yorumu beklenir (`missing_docs`).
- `ferrobridge-ir` başka hiçbir ferrobridge crate'ine bağımlı olamaz. İç bağımlılıklar ancak kullanılınca eklenir; hedef grafik kök `Cargo.toml`'da yorum olarak yazılı.
