# Releasing ferrobridge

All seven published crates share one version and are released together (ADR-0012). Publishing to crates.io cannot be undone; a version can only be yanked. Follow the steps in order.

## 1. Prepare

1. Choose the version. On `0.x`: a change to generated code or to the runtime's public API is a minor release (`0.1` → `0.2`); fixes that change neither are a patch release.
2. Set the version in the root `Cargo.toml`: `[workspace.package] version` **and** every exact pin in `[workspace.dependencies]` (`version = "=0.1.0"`).
3. Regenerate the committed e2e files (the generated header names the version; command in `crates/ferrobridge-e2e/README.md`) and update the codegen snapshots; review the diff by hand.
4. Move the `[Unreleased]` entries in `CHANGELOG.md` under the new version with today's date; update the comparison links at the bottom.
5. Update the version in the README snippets if the minor version changed (`ferrobridge = "0.x"`).

## 2. Check

```bash
export LIBCLANG_PATH="$(brew --prefix llvm)/lib"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo +1.85 check --workspace --all-targets --all-features
cargo deny check
cargo package --workspace --exclude ferrobridge-e2e
ARDUPILOT_DIR=~/ardupilot cargo test --manifest-path demo/ap_math/Cargo.toml
```

**Quickstart outside the repository.** In an empty directory, create a crate with a small C++ header and follow only the root README: install the command with `cargo install --path <repo>/crates/ferrobridge-cli --root <tmp>`, generate the bridge, depend on `ferrobridge` and `ferrobridge-build` (pointed at the repository with `[patch.crates-io]`), build and run. Do it once with committed files and once with the `generate` feature.

CI must be green on the release commit.

## 3. Publish

The crates.io account needs a verified email address. crates.io limits how many **new** crates can be published in a short time (a version of an existing crate is not affected): with new crates, expect HTTP 429 after about five and retry each remaining crate when the time in the error has passed (`cargo publish -p <crate>`).

```bash
cargo login                                  # once per machine; the token stays on your machine
cargo publish --workspace --dry-run
cargo publish --workspace                    # publishes in dependency order
git tag -a v0.1.0 -m "ferrobridge 0.1.0"
git push origin v0.1.0
```

Create a GitHub release from the tag with the CHANGELOG section as its text.

## 4. After publishing

- Check that docs.rs built `ferrobridge` and `ferrobridge-build` (and the internal crates).
- Repeat the quickstart without `[patch.crates-io]`, against the published versions, with a clean `CARGO_HOME`, including `cargo install ferrobridge-cli`.
- Add an empty `[Unreleased]` section to `CHANGELOG.md` if it is missing.

## Yanking

If a published version is broken (it does not build, or generated code is unsound), publish a fixed patch release first, then yank the broken version for every crate: `cargo yank --version 0.1.0 <crate>`. Yanking does not break existing lockfiles; it only stops new resolutions. Note the yank in `CHANGELOG.md`.
