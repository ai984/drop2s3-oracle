---
name: rust-macros
description: Comprehensive guide to Rust macros - declarative (macro_rules!) and procedural (derive, attribute, function-like). Covers macro hygiene, best practices, and common patterns. Use when creating macros, understanding macro expansion, or reducing boilerplate. Triggers on macro_rules!, proc_macro, derive macro, or metaprogramming questions.
---

# Rust Macros Best Practices

You are an expert in Rust macros, both declarative and procedural.

## When to Use Macros

**Use macros when:**
- You need compile-time code generation
- Functions cannot express the pattern (variadic arguments, syntax manipulation)
- You want to reduce repetitive boilerplate
- Building DSLs (Domain-Specific Languages)

**Prefer functions when:**
- A regular function can do the job
- You don't need compile-time evaluation
- Code clarity is more important than convenience

## Declarative Macros (macro_rules!)

### Basic Syntax

```rust
macro_rules! say_hello {
    // No arguments
    () => {
        println!("Hello!");
    };
    // With argument
    ($name:expr) => {
        println!("Hello, {}!", $name);
    };
}

say_hello!();           // "Hello!"
say_hello!("World");    // "Hello, World!"
```

### Fragment Specifiers

| Specifier | Matches | Example |
|-----------|---------|---------|
| `$x:expr` | Expression | `1 + 2`, `foo()` |
| `$x:ident` | Identifier | `my_var`, `MyStruct` |
| `$x:ty` | Type | `i32`, `Vec<String>` |
| `$x:pat` | Pattern | `Some(x)`, `_` |
| `$x:path` | Path | `std::io::Error` |
| `$x:stmt` | Statement | `let x = 1;` |
| `$x:block` | Block | `{ ... }` |
| `$x:item` | Item | `fn foo() {}` |
| `$x:meta` | Meta item | `derive(Debug)` |
| `$x:tt` | Token tree | Any single token or `(...)`, `[...]`, `{...}` |
| `$x:literal` | Literal | `42`, `"hello"` |
| `$x:lifetime` | Lifetime | `'a`, `'static` |
| `$x:vis` | Visibility | `pub`, `pub(crate)` |

### Repetition Patterns

```rust
macro_rules! vec_of_strings {
    // Zero or more (*), one or more (+), zero or one (?)
    ($($element:expr),* $(,)?) => {
        {
            let mut v = Vec::new();
            $(
                v.push($element.to_string());
            )*
            v
        }
    };
}

let v = vec_of_strings!["a", "b", "c"];  // Vec<String>
let v = vec_of_strings!["a", "b", "c",]; // Trailing comma allowed
```

### Multiple Arms with Different Patterns

```rust
macro_rules! create_function {
    // No return type
    ($name:ident) => {
        fn $name() {
            println!("Function {} called", stringify!($name));
        }
    };
    // With return type
    ($name:ident -> $ret:ty) => {
        fn $name() -> $ret {
            Default::default()
        }
    };
    // With body
    ($name:ident -> $ret:ty { $($body:tt)* }) => {
        fn $name() -> $ret {
            $($body)*
        }
    };
}

create_function!(foo);              // fn foo() { ... }
create_function!(bar -> i32);       // fn bar() -> i32 { 0 }
create_function!(baz -> i32 { 42 }); // fn baz() -> i32 { 42 }
```

### Recursive Macros

```rust
macro_rules! count {
    () => (0);
    ($head:tt $($tail:tt)*) => (1 + count!($($tail)*));
}

assert_eq!(count!(a b c d), 4);

// Creating nested structures
macro_rules! nested_vec {
    ($($element:expr),* $(,)?) => {
        vec![$($element),*]
    };
    ([$($inner:tt)*] $(, $($rest:tt)*)?) => {
        vec![nested_vec!($($inner)*) $(, nested_vec!($($rest)*))?]
    };
}
```

### Internal Rules (for complex macros)

```rust
macro_rules! complex_macro {
    // Public entry point
    ($($input:tt)*) => {
        complex_macro!(@internal start; $($input)*)
    };
    
    // Internal rules (prefixed with @)
    (@internal start; $first:expr, $($rest:tt)*) => {
        complex_macro!(@internal process; $first; $($rest)*)
    };
    
    (@internal process; $acc:expr; $next:expr, $($rest:tt)*) => {
        complex_macro!(@internal process; $acc + $next; $($rest)*)
    };
    
    (@internal process; $acc:expr;) => {
        $acc
    };
}
```

## Procedural Macros

### Project Setup

Proc macros must be in a separate crate:

```toml
# my_macro/Cargo.toml
[package]
name = "my_macro"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full"] }
quote = "1"
proc-macro2 = "1"
```

### Derive Macro

```rust
// my_macro/src/lib.rs
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields};

#[proc_macro_derive(MyTrait)]
pub fn derive_my_trait(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    
    let expanded = quote! {
        impl MyTrait for #name {
            fn describe(&self) -> String {
                format!("This is a {}", stringify!(#name))
            }
        }
    };
    
    TokenStream::from(expanded)
}

// With attributes
#[proc_macro_derive(MyTrait, attributes(my_attr))]
pub fn derive_with_attrs(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    
    // Process attributes
    for attr in &input.attrs {
        if attr.path().is_ident("my_attr") {
            // Handle attribute
        }
    }
    
    // ...
}
```

### Attribute Macro

```rust
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn log_calls(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_block = &input.block;
    let fn_sig = &input.sig;
    let fn_vis = &input.vis;
    let fn_attrs = &input.attrs;
    
    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            println!("Entering {}", stringify!(#fn_name));
            let result = (|| #fn_block)();
            println!("Exiting {}", stringify!(#fn_name));
            result
        }
    };
    
    TokenStream::from(expanded)
}

// Usage:
// #[log_calls]
// fn my_function() { ... }
```

### Function-like Macro

```rust
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr, punctuated::Punctuated, Token, Expr};

#[proc_macro]
pub fn sql(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);
    let sql_string = input.value();
    
    // Validate SQL at compile time
    if !sql_string.to_uppercase().starts_with("SELECT") {
        return syn::Error::new(input.span(), "Only SELECT queries allowed")
            .to_compile_error()
            .into();
    }
    
    let expanded = quote! {
        Query::new(#sql_string)
    };
    
    TokenStream::from(expanded)
}

// Usage:
// let query = sql!("SELECT * FROM users");
```

### Working with Struct Fields

```rust
use syn::{Data, Fields, Type};

fn process_struct(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;
    
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            Fields::Unnamed(fields) => &fields.unnamed,
            Fields::Unit => return quote! { /* unit struct */ }.into(),
        },
        _ => panic!("Only structs are supported"),
    };
    
    let field_names: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let field_types: Vec<_> = fields.iter().map(|f| &f.ty).collect();
    
    let expanded = quote! {
        impl #name {
            pub fn field_names() -> &'static [&'static str] {
                &[#(stringify!(#field_names)),*]
            }
        }
    };
    
    expanded.into()
}
```

### Error Handling in Proc Macros

```rust
use syn::spanned::Spanned;

fn validate_input(input: &DeriveInput) -> syn::Result<()> {
    match &input.data {
        Data::Struct(_) => Ok(()),
        Data::Enum(e) => Err(syn::Error::new(
            e.enum_token.span(),
            "MyTrait cannot be derived for enums"
        )),
        Data::Union(u) => Err(syn::Error::new(
            u.union_token.span(),
            "MyTrait cannot be derived for unions"
        )),
    }
}

#[proc_macro_derive(MyTrait)]
pub fn derive_my_trait(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    
    match validate_input(&input) {
        Ok(()) => generate_impl(&input),
        Err(e) => e.to_compile_error().into(),
    }
}
```

## Best Practices

### 1. Use `$crate` for Hygiene

```rust
macro_rules! my_vec {
    ($($elem:expr),*) => {
        {
            // Use $crate to reference items from this crate
            let mut v = $crate::collections::MyVec::new();
            $(v.push($elem);)*
            v
        }
    };
}
```

### 2. Accept Trailing Commas

```rust
macro_rules! list {
    ($($item:expr),* $(,)?) => {  // Note the $(,)?
        // ...
    };
}

list![1, 2, 3];   // Works
list![1, 2, 3,];  // Also works
```

### 3. Provide Good Error Messages

```rust
macro_rules! require_struct {
    (struct $name:ident { $($fields:tt)* }) => {
        // Process struct
    };
    ($($other:tt)*) => {
        compile_error!("Expected a struct definition");
    };
}
```

### 4. Document Your Macros

```rust
/// Creates a HashMap with the given key-value pairs.
///
/// # Example
///
/// ```
/// let map = hashmap! {
///     "a" => 1,
///     "b" => 2,
/// };
/// ```
#[macro_export]
macro_rules! hashmap {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            let mut map = ::std::collections::HashMap::new();
            $(map.insert($key, $value);)*
            map
        }
    };
}
```

### 5. Test Macro Expansion

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_macro() {
        let result = my_macro!(some input);
        assert_eq!(result, expected);
    }
}

// For proc macros, use trybuild for compile-fail tests
// Cargo.toml: trybuild = "1"
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass/*.rs");
    t.compile_fail("tests/ui/fail/*.rs");
}
```

### 6. Use `cargo expand` for Debugging

```bash
cargo install cargo-expand
cargo expand
cargo expand my_module::my_function
```

## Common Patterns

### Builder Pattern with Macro

```rust
macro_rules! builder {
    ($name:ident { $($field:ident: $type:ty),* $(,)? }) => {
        pub struct $name {
            $($field: Option<$type>),*
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    $($field: None),*
                }
            }

            $(
                pub fn $field(mut self, value: $type) -> Self {
                    self.$field = Some(value);
                    self
                }
            )*

            pub fn build(self) -> Result<Built, &'static str> {
                Ok(Built {
                    $($field: self.$field.ok_or(concat!(stringify!($field), " is required"))?),*
                })
            }
        }

        pub struct Built {
            $($field: $type),*
        }
    };
}

builder!(PersonBuilder {
    name: String,
    age: u32,
});
```

### Enum Dispatch

```rust
macro_rules! dispatch {
    ($enum:ident, $self:expr, $method:ident $(, $args:expr)*) => {
        match $self {
            $enum::A(inner) => inner.$method($($args),*),
            $enum::B(inner) => inner.$method($($args),*),
            $enum::C(inner) => inner.$method($($args),*),
        }
    };
}
```

## References

- [The Little Book of Rust Macros](https://veykril.github.io/tlborm/)
- [Rust Reference - Macros](https://doc.rust-lang.org/reference/macros.html)
- [syn Documentation](https://docs.rs/syn)
- [quote Documentation](https://docs.rs/quote)
- [proc-macro-workshop](https://github.com/dtolnay/proc-macro-workshop)
