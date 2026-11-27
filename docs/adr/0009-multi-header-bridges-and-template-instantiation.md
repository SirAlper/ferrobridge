# ADR-0009: Multi-header bridges and template instantiation

**Date**: 2026-11-27
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

In real C++ libraries types are spread over several headers. In `AP_Math`, `Vector2f` is defined in `vector2.h`; the functions using it are in `control.h`, `location.h` and `SCurve.h`. Until M5 each header was bound separately, and every function using a type from another header was skipped. The library's core types (`Vector3<T>`, `Matrix3<T>`, `QuaternionT<T>`) are also class templates. The PRD had left templates out of the MVP, but the M5 measurement showed that the 80% goal could not be reached without them.

## Decision

1. **One bridge is generated from several headers.** CLI: `ferrobridge generate a.h b.h ...`; build: `Generate::new("name").header(..)`.
   - The parser reads them all in a single synthetic translation unit. Analysis recognizes types across all headers.
   - A type from a header not added to the bridge is skipped, but the message names that header (`declared_in` in the IR).
2. **Class templates are instantiated only with the concrete types the user lists:**
   ```toml
   [templates]
   Vector3f = "Vector3<float>"
   ```
   - libclang resolves the instantiation's arguments and layout. A `using` alias and a `static_assert(sizeof(...))` that forces the class to be instantiated are added to the synthetic unit.
   - Because libclang does not show the members of an instantiated class, members are read from the template's pattern. `type-parameter-0-N` in the pattern is replaced with the concrete argument.
   - A dependent reference (`Vector2<T>`) is tied to an instantiation if the list has one with those arguments. Every unrecognized dependent type becomes `Unsupported`.
   - On the C++ side, the shim defines the alias `using Vector3f = ::Vector3<float>;`. It does **not** do an explicit instantiation; that would clash with the library's own `template class ...;`.
3. **Scope:** only class templates with type parameters. Non-type parameters, member templates and function templates are rejected with a reason.

## Alternatives considered

### Alternative 1: sharing types between separate bridges
- **Pros**: Each header stays in its own module.
- **Cons**: In cxx, another bridge's type is imported with an `ExternType` alias. Users would have to manage dependencies between bridges; circular references (two headers using each other's types) cannot be resolved.
- **Why not chosen**: A single multi-header bridge gives the same result without burdening the user.

### Alternative 2: finding all instantiations used in the header automatically
- **Pros**: No configuration needed.
- **Cons**: Not every `Vector3<int16_t>` mentioned in a header has members defined in the library (only those instantiated explicitly in a `.cpp` can be linked); link errors follow. Which instantiation to bind would be a guess.
- **Why not chosen**: Safety rule 1 (ask, don't guess).

### Alternative 3: walking libclang's instantiated class (including explicit instantiations)
- **Why not chosen**: Not possible. libclang does not expose the members of instantiations as cursors (verified in M6 Task 0).

## Consequences

### Positive
- In `AP_Math`, `Vector2f/3f`, `Matrix3f`, `Quaternion` and their `double` versions are bound; their uses in other headers open up as well.
- A single `ferrobridge.toml` is enough for the whole bridge.

### Negative
- The substitution engine is new and complex code. A wrong substitution is a compile error, not silent UB: the shim and cxx produce typed C++ signatures.
- The user must know which instantiations are defined in the library. If the members of a listed instantiation are not compiled, the user gets a link error.

### Risks
- The pattern's representation in libclang may change between versions (e.g. the spelling of `type-parameter-0-N`). Mitigation: the same snapshots on two libclang versions (Apple 21, Homebrew 23).
