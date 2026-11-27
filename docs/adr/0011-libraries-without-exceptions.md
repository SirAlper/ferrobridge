# ADR-0011: Libraries without exceptions (`[library] exceptions = false`)

**Date**: 2026-11-27
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

Per safety rule 4, exceptions do not cross the FFI boundary: every C++ function that is not `noexcept` returns `Result` in Rust. Embedded libraries are often compiled with `-fno-exceptions` and do not write `noexcept` on their functions. ArduPilot's waf settings, for example, use it, and `AP_Math` has no `throw`. In such a library every call returning `Result` (`v.length()?`) makes the API noisy and carries no information. This was an open question in the PRD.

## Decision

In `ferrobridge.toml`, this is a promise: "no function of this library throws an exception".

```toml
[library]
exceptions = false
```

- If the promise is made, all functions are treated as `noexcept` and return values directly.
- ferrobridge's own checks stay `Result`: slice length overflow, null `borrowed` return, undefined enum value, UTF-8.
- **A broken promise does not cause undefined behavior.** cxx makes the C++ wrapper of functions that do not return `Result` `noexcept`; the shim is `noexcept` too. If an exception is thrown, the program ends with `std::terminate`. This was verified in M6 Task 0 and in e2e with a child process.
- The promise is added to the generated files' "Assumptions based on annotations" list.

## Alternatives considered

### Alternative 1: a per-function `noexcept = true` annotation
- **Why not chosen**: It would need hundreds of lines for a property that holds library-wide. Can be added later for exceptions to the rule.

### Alternative 2: detecting the `-fno-exceptions` compile flag automatically
- **Cons**: The flag belongs to the library's own build. It cannot be derived from the bridge's build and is not certain.
- **Why not chosen**: It would be a guess (rule 1).

## Consequences

### Positive
- In the demo, calls like `a.length()` and `ap::degrees(x)` without `?`.
- `std::ops` traits can be generated for these libraries too; trait methods cannot return `Result`.

### Negative
- If the promise is wrong, the program ends instead of returning an error. A safe failure, chosen deliberately over UB.
