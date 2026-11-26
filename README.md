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

## Kullanım

Bir header'ın içinde ne bulunduğunu görmek için:

```bash
cargo run -p ferrobridge-cli -- parse tests/fixtures/free_functions/input.h
```

Çıktı, header'daki serbest fonksiyonları (namespace, parametre ve tipleriyle) içeren girintili JSON'dur. Henüz desteklenmeyen yapılar (sınıf, enum, template...) atlanmaz; `"kind": "unsupported"` öğesi olarak nedenleriyle listelenir. clang'a ek argüman vermek için `--` kullanın:

```bash
cargo run -p ferrobridge-cli -- parse include/mylib.h --std c++20 -- -I include -DMYLIB_EXPORT=
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

Test fixture'ları `tests/fixtures/<konu>/input.h` altında, beklenen çıktılar `crates/ferrobridge-parser/tests/snapshots/` altında [insta](https://insta.rs) snapshot'ı olarak tutulur. Çıktı değiştiğinde `cargo insta review` ile inceleyin.

## Belgeler

- Gereksinimler: [`.claude/prds/ferrobridge.prd.md`](.claude/prds/ferrobridge.prd.md)
- Mimari kararlar: [`docs/adr/`](docs/adr/README.md)

## Lisans

ferrobridge açık kaynaktır ve iki lisansın birini seçebileceğiniz şekilde dağıtılır:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

### Katkı

Aksi açıkça belirtilmedikçe, Apache-2.0 lisansında tanımlandığı şekliyle bu projeye dahil edilmek üzere bilerek gönderdiğiniz her katkı, ek şart veya koşul olmadan yukarıdaki gibi çift lisanslanır.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
