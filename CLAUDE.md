# ferrobridge

C++ kütüphanelerini Rust'tan **güvenli ve idiomatik** şekilde kullanmayı sağlayan bağlama (binding) üreteci.
libclang ile header okur → ara model (IR) → ownership çıkarımı + `ferrobridge.toml` açıklamaları → `cxx` bridge + C++ shim + güvenli Rust API üretir.

Açık kaynak: `MIT OR Apache-2.0` (bkz. `docs/adr/0003-open-source-dual-license.md`).

Durum: **Milestone 3 (sahiplik açıklamaları) tamamlandı**: `ferrobridge generate <header> --out-dir <dizin> [--config ferrobridge.toml]` sınıflar, metotlar, enum'lar, `std::string`, `std::vector` ve `ferrobridge.toml`'da açıklanan pointer'lar için cxx bridge + C++ shim üretiyor; `ferrobridge suggest <header>` açıklama bekleyen yerler için taslak basıyor; `crates/ferrobridge-e2e` bunu gerçek C++ ile test ediyor. Sırada Milestone 4: güvenli API (`/ecc:plan .claude/prds/ferrobridge.prd.md`).

## Belgeler
- Gereksinimler (PRD): `.claude/prds/ferrobridge.prd.md`. Planlama komutu `/ecc:plan .claude/prds/ferrobridge.prd.md` bu dosyayı okur.
- Mimari kararlar: `docs/adr/` (dizin: `docs/adr/README.md`, şablon: `docs/adr/template.md`). Kabul edilmiş ADR'ler: 0001 (libclang via `clang-sys`), 0002 (cxx üstüne inşa), 0003 (açık kaynak çift lisans), 0004 (üretilen kod commit'lenir), 0005 (overload/çakışma adlandırma), 0006 (sahiplik açıklamaları ayrı TOML dosyasında).
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
```

Üretici veya fixture'lar değişince `crates/ferrobridge-e2e/src/generated/` yeniden üretilmeli (komut: `crates/ferrobridge-e2e/README.md`; `ferrobridge.toml`'u olan fixture'lar `--config` ile); aksi halde `generated_files_match_the_committed_e2e_files` testi başarısız olur. Üretilen `bridge.rs` `#[rustfmt::skip]` taşır, `cargo fmt` ona dokunmaz. Snapshot değişikliklerini `cargo insta review` ile **elle inceleyip** kabul et.

## Mimari (kısa)
`parser` (libclang → IR) → `analysis` (`ferrobridge.toml` + neyin güvenle bağlanacağı + Rust adları → `BridgePlan`) → `codegen` (`bridge.rs`, `shim.h`, `shim.cc`) → `cli`. Bütün `unsafe` `crates/ferrobridge-parser/src/clang.rs`'de.

## Kod kuralları (workspace lint'leriyle zorlanır)
- Testler dışında `unwrap()` / `expect()` yasak (`clippy.toml` testlerde izin verir).
- Her `unsafe` bloğun üstünde `// SAFETY:` gerekçesi zorunlu. Yeni libclang çağrıları yalnızca `clang.rs`'e eklenir.
- Integration test dosyalarında `#![allow(clippy::unwrap_used)]` gerekir (`clippy.toml` istisnası yardımcı fonksiyonları kapsamıyor).
- Public öğelerde doküman yorumu beklenir (`missing_docs`).
- `ferrobridge-ir` başka hiçbir ferrobridge crate'ine bağımlı olamaz. İç bağımlılıklar ancak kullanılınca eklenir; hedef grafik kök `Cargo.toml`'da yorum olarak yazılı.
