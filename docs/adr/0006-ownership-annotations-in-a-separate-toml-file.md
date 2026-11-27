# ADR-0006: Ownership annotations in a separate TOML file

**Date**: 2026-11-27
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

A C++ header does not say who owns a pointer or how long it lives. ferrobridge does not guess this (safety rule 1); the information has to come from the user. Most libraries to be bound are third-party (the first target is ArduPilot `AP_Math`, GPLv3; its code is not copied into the repo and its headers cannot be changed). A wrong annotation can make the generated code cause undefined behavior, so annotations must live in one place, in a reviewable form, and typos must not be silently ignored. This decision closes the PRD's open question about where annotations live.

## Decision

Ownership annotations are written in a separate `ferrobridge.toml` file next to the header and passed with `ferrobridge generate --config`. The key is the full C++ name (`ns::Class::method`, `ns::Class::Class` for a constructor); values are `returns = "owned" | "borrowed"` and `params.<name> = "borrowed"`. Every annotation is a **promise** made by the user: every promise used is listed at the top of the generated files under "Assumptions based on annotations"; an annotation with no counterpart in the header is an error, an unnecessary one is a warning. `ferrobridge suggest` prints a draft with commented-out lines for the places that need annotations; the user enables the values deliberately.

## Alternatives considered

### Alternative 1: a C++ attribute in the header (`[[clang::annotate("fb::owned")]]`)
- **Pros**: The information sits right next to the declaration and changes with the header.
- **Cons**: Third-party headers cannot be edited; even in one's own code, unknown-attribute warnings on some compilers.
- **Why not chosen**: Unusable in the first target scenario (ArduPilot). Can be added in v0.2 as an extra source for people binding their own code; the separate file stays valid.

### Alternative 2: writing annotations under `[package.metadata.ferrobridge]` in `Cargo.toml`
- **Pros**: No extra file.
- **Cons**: CLI use becomes tied to Cargo; several headers get mixed in one file; errors with line numbers get harder.
- **Why not chosen**: Annotations are a header's contract, not the package's. The `build.rs` integration (`ferrobridge-build`) can take the path of the same TOML file.

### Alternative 3: printing the `suggest` draft with values filled in (e.g. the first option)
- **Pros**: The user writes less.
- **Cons**: Accepting the draft as is would mean guessing ownership; a wrong option is UB.
- **Why not chosen**: Safety rule 1. Values are comments; the item of a line that is not enabled stays unbound.

## Consequences

### Positive
- Third-party libraries can be bound without touching the header.
- The points of trust are in one file and at the top of every generated file; visible at a glance in code review.
- Typos (unknown key/value, nonexistent function/parameter) are errors with line numbers.

### Negative
- When the header changes (e.g. a parameter name), the annotation file must be updated too; the unknown-name error catches this.
- A key applies to **all overloads** with that name; annotating overloads separately (signature keys) is left to v0.2.

### Risks
- The user makes a wrong promise (e.g. an object declared `owned` was not allocated with `new`) → UB. Mitigation: the list of assumptions, a clear warning in the README, and the cheap checks are always done (null on a borrowed return → `Err`; on an owned return, is the destructor public, and virtual in a polymorphic class).
- An annotation applies to several overloads and is wrong for one of them. Mitigation: in this case a warning is printed with the signatures of the overloads it applies to.
