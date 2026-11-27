# ADR-0005: Naming rule for overloads and name collisions

**Date**: 2026-11-27
**Status**: accepted
**Deciders**: Alper (project owner)

## Context

C++ allows function overloading and the same name in different namespaces; in a cxx bridge, Rust names must be unique (all items of the bridge live in a single Rust module). C++ names can also be Rust keywords (`type`, `move`), and because cxx copies Rust parameter names into the generated C++ code, invented names must not be C++ keywords (in a cxx experiment, a parameter named `this` caused a compile error). The rule must be deterministic and explainable.

## Decision

Rust names are assigned by the following rules, in order; the C++ name is always kept with `#[cxx_name]`:

1. A name defined once stays as it is.
2. If the same C++ name exists in more than one namespace, the namespace path becomes a prefix (`geo_length`, `physics_length`). For types, PascalCase (`PhysicsVehicle`).
3. **All** overloads sharing a name get a suffix derived from the parameter types (`clamp_i32_i32_i32`, `clamp_f64_f64_f64`; `noargs` for no parameters; `mut_T` for `Pin<&mut T>`).
4. Constructors become `<Class>_new` and static methods `<Class>_<method>` (with the same rules).
5. Rust keywords are escaped with `_` (`type_`); unnamed parameters become `argN`. The receiver parameter in shims is `fb_self` (not a C++ keyword).
6. Names that still collide after this get a sequence number (`_2`).

## Alternatives considered

### Alternative 1: the first overload stays plain, the others are numbered (`clamp`, `clamp_2`)
- **Pros**: Short names.
- **Cons**: A number does not say what the overload is; the meaning changes when the order in the header changes.
- **Why not chosen**: A name derived from the types explains itself and is independent of order.

### Alternative 2: not binding overloads at all
- **Pros**: No naming problem.
- **Cons**: Most real C++ APIs use overloads.
- **Why not chosen**: The PRD's coverage goal (80%) could not be reached.

### Alternative 3: the user names every overload
- **Pros**: The most readable names.
- **Cons**: Does not work without an annotation file; friction on first use.
- **Why not chosen**: An automatic rule by default, together with name overrides in `ferrobridge.toml` later, is best.

## Consequences

### Positive
- Names are deterministic and readable from the types.
- Snapshot tests pin the names; an unexpected name change shows up in the diff.

### Negative
- **Deliberate cost:** when a new overload is added to the header, the name of the formerly unique function changes too (`area` → `area_noargs`). This breaks user code.
- Names may not follow Rust style (`Vehicle_new_i32`); the `non_snake_case` warning is turned off in the generated module. Idiomatic names are the job of the wrapper layer in M4.

### Risks
- **Name drift breaks user code.** Mitigation: the safe API layer in M4 and name overrides in `ferrobridge.toml` will give users stable names.
