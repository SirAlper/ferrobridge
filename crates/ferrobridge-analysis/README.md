# ferrobridge-analysis

Decides what ferrobridge can bind safely: it reads the ownership annotations in `ferrobridge.toml`, applies the safety rules (no guessing about raw pointers), chooses Rust names and produces the bridge plan.

**This crate is an implementation detail of [ferrobridge](https://github.com/SirAlper/ferrobridge).** Its API has no stability guarantee and may change in any release. Use the `ferrobridge` command (`cargo install ferrobridge-cli`) or [`ferrobridge-build`](https://crates.io/crates/ferrobridge-build) instead.

All ferrobridge crates are released together with the same version (ADR-0012).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
