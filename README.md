# ferrobridge

C++ header'larından **güvenli ve idiomatik** Rust binding'leri üreten araç. cxx üzerine kuruludur, sahiplik bilgisini bir açıklama dosyasından (`ferrobridge.toml`) alır ve Bazel gerektirmez.

> Durum: erken geliştirme (Faz 0). Henüz kullanılabilir bir sürüm yok.

## Gereksinimler

- Rust (edition 2024, en az 1.85)
- LLVM / libclang

```bash
brew install llvm
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
```

Ortamı kontrol etmek için:

```bash
cargo run -p ferrobridge-cli -- doctor
```

## Workspace

| Crate | Görev |
|---|---|
| `ferrobridge-ir` | Dilden bağımsız ara model (başka ferrobridge crate'ine bağımlı değil) |
| `ferrobridge-parser` | libclang ile C++ header okuma |
| `ferrobridge-analysis` | Sahiplik çıkarımı, açıklama dosyası, tanı mesajları |
| `ferrobridge-codegen` | cxx bridge, C++ shim ve güvenli Rust API üretimi |
| `ferrobridge-build` | `build.rs` içinden çağrılan API |
| `ferrobridge-cli` | `ferrobridge` komut satırı aracı |

Test fixture'ları `tests/fixtures/` altında tutulur.

## Belgeler

- Gereksinimler: [`.claude/prds/ferrobridge.prd.md`](.claude/prds/ferrobridge.prd.md)
- Mimari kararlar: [`docs/adr/`](docs/adr/README.md)
