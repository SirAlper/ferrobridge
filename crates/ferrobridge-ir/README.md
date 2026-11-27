# ferrobridge-ir

The intermediate model (IR) of the C++ declarations ferrobridge reads: functions, classes, enums, constants, types and source locations. It serializes to JSON (`ferrobridge parse`) and depends on no other ferrobridge crate.

**This crate is an implementation detail of [ferrobridge](https://github.com/SirAlper/ferrobridge).** Its API has no stability guarantee and may change in any release. Use the `ferrobridge` command (`cargo install ferrobridge-cli`) or [`ferrobridge-build`](https://crates.io/crates/ferrobridge-build) instead.

All ferrobridge crates are released together with the same version (ADR-0012).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
