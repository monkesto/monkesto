# Macro Design Principles

Macros should stay conservative. They are most useful when they make a precise
pattern easier to use correctly without hiding the public API that application
code depends on.

## Principles

- Public types must be named explicitly by the macro caller. If application code
  can construct it, pattern match it, return it, or mention it in a bound, the
  macro invocation should name it.

- Public types should look like new Rust types in the macro declaration. A Rust
  developer should be able to guess the important generated type names by reading
  the macro call.

- Macros should not invent public API names by convention. Hidden helper types or
  trait wiring are fine when users are not expected to name them directly.

- Avoid macros unless they provide a significant benefit over writing the structs
  and trait impls directly.

- Good reasons for a macro include:
  - Enforcing a precise wiring pattern that is easy to get subtly wrong.
  - Generating repetitive impls across a variadic list of types.
  - Presenting a language feature Rust does not have directly, such as variadic
    type parameters in a position where they would materially improve the API.

- Bad reasons for a macro include:
  - Saving a few lines of straightforward code.
  - Hiding important domain types from search and navigation.
  - Making the generated API harder to infer than direct Rust declarations.

## Example

This is a good shape because every public type is named directly by the caller:

```rust
family! {
    pub struct AuthorizationFamily {
        type Id = AuthorizationId;
        type Record = AuthorizationRecord;

        Role(RoleMember),
        Grant(GrantMember),
    }
}
```

The macro may generate hidden helper impls to wire these types together. It
should not secretly create public names like `AuthorizationId` or
`AuthorizationRecord`.
