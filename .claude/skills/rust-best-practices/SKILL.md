---
name: rust-best-practices
description: Expert Rust development guidelines for idiomatic code, async programming, error handling, and performance. Use when writing, reviewing, or refactoring Rust code. Triggers on Rust files (*.rs), Cargo.toml, or Rust-related questions.
---

# Rust Development Best Practices

You are an expert in Rust, async programming, and concurrent systems.

## Key Principles

- Write clear, concise, and idiomatic Rust code with accurate examples
- Use async programming paradigms effectively, leveraging `tokio` for concurrency
- Prioritize modularity, clean code organization, and efficient resource management
- Use expressive variable names that convey intent (e.g., `is_ready`, `has_data`)
- Adhere to Rust's naming conventions: `snake_case` for variables and functions, `PascalCase` for types and structs
- Avoid code duplication; use functions and modules to encapsulate reusable logic
- Write code with safety, concurrency, and performance in mind, embracing Rust's ownership and type system
- Follow the Rust API Guidelines for public APIs
- Prefer `impl Trait` over explicit generics where appropriate
- Use `#[must_use]` for functions that return important values

## Error Handling (CRITICAL)

- Embrace Rust's `Result<T, E>` and `Option<T>` types for error handling
- Use `?` operator to propagate errors cleanly
- Implement custom error types using `thiserror` for library code
- Use `anyhow` for application-level error handling with context
- Handle errors and edge cases early, returning errors where appropriate
- **NEVER use `.unwrap()` in library code** - only acceptable in tests or when logically impossible to fail
- Use `.expect("reason")` sparingly with clear explanation when panic is intentional
- Prefer `if let` or `match` over `.unwrap_or_default()` when the default needs thought

```rust
// GOOD: Proper error handling
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[error("Failed to read config: {0}")]
    ConfigError(#[from] std::io::Error),
    #[error("Invalid input: {reason}")]
    ValidationError { reason: String },
}

fn process(data: &str) -> Result<Output, MyError> {
    let config = read_config()?;  // Propagate with ?
    validate(data).map_err(|e| MyError::ValidationError { reason: e })?;
    Ok(output)
}

// BAD: Don't do this
fn process_bad(data: &str) -> Output {
    let config = read_config().unwrap();  // NEVER in library code
    // ...
}
```

## Async Programming with Tokio

- Use `tokio` as the async runtime for handling asynchronous tasks and I/O
- Implement async functions using `async fn` syntax
- Leverage `tokio::spawn` for task spawning and concurrency
- Use `tokio::select!` for managing multiple async tasks and cancellations
- Favor structured concurrency: prefer scoped tasks and clean cancellation paths
- Implement timeouts, retries, and backoff strategies for robust async operations
- Use `.await` responsibly, ensuring safe points for context switching

```rust
use tokio::time::{timeout, Duration};

async fn fetch_with_timeout(url: &str) -> Result<Response, Error> {
    timeout(Duration::from_secs(30), fetch(url))
        .await
        .map_err(|_| Error::Timeout)?
}
```

## Channels and Concurrency

- Use `tokio::sync::mpsc` for asynchronous, multi-producer, single-consumer channels
- Use `tokio::sync::broadcast` for broadcasting messages to multiple consumers
- Implement `tokio::sync::oneshot` for one-time communication between tasks
- Prefer bounded channels for backpressure; handle capacity limits gracefully
- Use `tokio::sync::Mutex` and `tokio::sync::RwLock` for shared state across tasks
- Avoid deadlocks by consistent lock ordering

```rust
use tokio::sync::mpsc;

async fn worker(mut rx: mpsc::Receiver<Task>) {
    while let Some(task) = rx.recv().await {
        process(task).await;
    }
}
```

## Ownership and Borrowing

- Prefer borrowing (`&T`, `&mut T`) over ownership when possible
- Use `Cow<'_, str>` when you might or might not need to own data
- Avoid unnecessary clones; use references or `Arc` for shared ownership
- Understand and leverage the borrow checker - it's your friend
- Use `std::mem::take` or `std::mem::replace` for moving out of mutable references

## Type System Best Practices

- Use the newtype pattern for type safety (e.g., `struct UserId(u64)`)
- Prefer enums over boolean flags for state representation
- Use `PhantomData` for type-level markers
- Leverage trait bounds for generic constraints
- Use associated types for "output" types in traits

```rust
// GOOD: Type-safe IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OrderId(pub u64);

// Compiler prevents mixing them up!
fn get_user_orders(user_id: UserId) -> Vec<OrderId> { ... }
```

## Derive Macros and Traits

Always derive appropriate traits for your types:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]  // For value types
pub struct Config { ... }

#[derive(Debug, Clone, Serialize, Deserialize)]  // For data transfer
pub struct ApiResponse { ... }

#[derive(Debug, thiserror::Error)]  // For errors
pub enum MyError { ... }
```

## Testing

- Write unit tests with `#[tokio::test]` for async tests
- Use `tokio::time::pause` for testing time-dependent code without real delays
- Implement integration tests to validate async behavior and concurrency
- Use mocks and fakes for external dependencies in tests
- Place unit tests in the same file with `#[cfg(test)]` module
- Use `proptest` or `quickcheck` for property-based testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_with_timeout() {
        tokio::time::pause();
        // Test without real delays
    }

    #[test]
    fn test_validation() {
        assert!(validate("valid").is_ok());
        assert!(validate("").is_err());
    }
}
```

## Performance Optimization

- Minimize async overhead; use sync code where async is not needed
- Avoid blocking operations inside async functions; offload to `tokio::task::spawn_blocking`
- Use `tokio::task::yield_now` to yield control in cooperative multitasking scenarios
- Optimize data structures and algorithms for async use, reducing contention
- Use `tokio::time::sleep` and `tokio::time::interval` for efficient time-based operations
- Prefer `Vec` over `LinkedList`, `HashMap` over `BTreeMap` unless ordering needed
- Use iterators and avoid collecting intermediate results when possible

```rust
// GOOD: Streaming without intermediate collection
data.iter()
    .filter(|x| x.is_valid())
    .map(|x| x.transform())
    .take(10)
    .collect()

// BAD: Unnecessary intermediate allocation
let filtered: Vec<_> = data.iter().filter(|x| x.is_valid()).collect();
let mapped: Vec<_> = filtered.iter().map(|x| x.transform()).collect();
```

## Code Organization

1. Structure the application into modules: separate concerns like networking, database, and business logic
2. Use `mod.rs` or inline modules appropriately
3. Keep `lib.rs` as the public API surface
4. Use `pub(crate)` for internal-only public items
5. Group related functionality in submodules

```
src/
├── lib.rs          # Public API
├── config.rs       # Configuration
├── error.rs        # Error types
├── db/
│   ├── mod.rs
│   ├── models.rs
│   └── queries.rs
└── api/
    ├── mod.rs
    ├── handlers.rs
    └── middleware.rs
```

## Tooling (MANDATORY)

- Format all code with `cargo fmt` before committing
- Lint with `cargo clippy` and fix all warnings
- Run `cargo check` frequently during development
- Use `cargo doc --open` to review your documentation
- Run `cargo test` before every commit
- Use `cargo audit` to check for security vulnerabilities in dependencies

## Common Clippy Lints to Follow

```rust
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]  // If needed

// Enable in Cargo.toml for stricter checks:
// [lints.clippy]
// all = "warn"
// pedantic = "warn"
```

## Async Ecosystem Libraries

- `tokio` - async runtime and task management
- `hyper` or `reqwest` - async HTTP requests
- `serde` + `serde_json` - serialization/deserialization
- `sqlx` or `tokio-postgres` - async database interactions
- `tonic` - gRPC with async support
- `tracing` - structured logging and diagnostics
- `tower` - middleware and service abstractions

## Documentation

- Use `///` doc comments for all public items
- Include examples in doc comments with ```` ```rust ````
- Document panics, errors, and safety requirements
- Use `#[doc(hidden)]` for internal implementation details
- Write a crate-level doc comment in `lib.rs`

```rust
/// Processes the input data and returns the result.
///
/// # Arguments
///
/// * `input` - The data to process
///
/// # Returns
///
/// The processed output, or an error if processing fails.
///
/// # Errors
///
/// Returns `ProcessError::InvalidInput` if the input is malformed.
///
/// # Examples
///
/// ```
/// let result = process("valid input")?;
/// assert!(result.is_valid());
/// ```
pub fn process(input: &str) -> Result<Output, ProcessError> {
    // ...
}
```

## Cargo.toml Best Practices

```toml
[package]
name = "my-crate"
version = "0.1.0"
edition = "2024"
rust-version = "1.75"  # Minimum supported version

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
thiserror = "1"
tracing = "0.1"

[dev-dependencies]
tokio-test = "0.4"
proptest = "1"

[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
```

## References

- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust Async Book](https://rust-lang.github.io/async-book/)
- [Tokio Documentation](https://tokio.rs/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Clippy Lints](https://rust-lang.github.io/rust-clippy/master/)
