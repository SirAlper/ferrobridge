# ADR-0003: Open source, dual licensed MIT OR Apache-2.0

**Date**: 2026-11-26
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

The project owner decided that ferrobridge will be fully open source. The license was an open question in the PRD. Target users will add ferrobridge to the build process of their own (often commercial) projects, so the license must not cause problems with companies' legal departments. Tools we build on, such as cxx and bindgen, use the standard license of the Rust ecosystem.

## Decision

All crates are distributed under the `MIT OR Apache-2.0` dual license. The root contains `LICENSE-MIT` and `LICENSE-APACHE`; the README contains the contribution clause common in Rust projects (contributions are accepted under the same dual license). Publishing to crates.io stays disabled with `publish = false` until v0.1.

## Alternatives considered

### Alternative 1: MIT only
- **Pros**: The shortest and simplest license.
- **Cons**: No explicit patent license.
- **Why not chosen**: Apache-2.0's patent clause matters to corporate users; the dual license offers both.

### Alternative 2: Apache-2.0 only
- **Pros**: Patent protection.
- **Cons**: Incompatible with GPLv2 projects.
- **Why not chosen**: We do not want to rule out use together with GPLv2 code bases.

### Alternative 3: copyleft (GPL / MPL-2.0)
- **Pros**: Keeps derivative works open too.
- **Cons**: The license status of code and libraries produced by a build tool becomes unclear to users; it makes adoption in companies harder.
- **Why not chosen**: The goal is for the tool to be used as widely as possible.

## Consequences

### Positive
- The same license as the Rust ecosystem (cxx, bindgen, Rust itself); no surprises for users.
- Open to corporate use and to GPLv2 projects.

### Negative
- Others can use the code in closed-source products and do not have to give their changes back.

### Risks
- **Licenses get mixed if third-party code and fixtures (e.g. ArduPilot `AP_Math`, GPLv3) are copied into the repo.** Mitigation: we **do not copy** GPL-licensed code into the repo; demos and tests use that code from outside (e.g. a checkout pointed to by an environment variable) and state its license separately. The small fixture headers we write ourselves are under the dual license.
