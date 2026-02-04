---
name: rust-testing
description: Comprehensive Rust testing guide covering unit tests, integration tests, async tests with tokio, property-based testing with proptest, mocking, and test organization. Use when writing tests, setting up test infrastructure, or debugging test failures. Triggers on test-related questions, #[test], proptest, or testing patterns.
---

# Rust Testing Best Practices

You are an expert in Rust testing strategies, from unit tests to property-based testing.

## Test Organization

### File Structure

```
src/
├── lib.rs
├── module.rs
└── module/
    └── submodule.rs
tests/                    # Integration tests
├── integration_test.rs
├── common/
│   └── mod.rs           # Shared test utilities
benches/                  # Benchmarks
└── benchmark.rs
```

### Unit Tests (Same File)

```rust
// src/calculator.rs
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn divide(a: i32, b: i32) -> Result<i32, &'static str> {
    if b == 0 {
        Err("Division by zero")
    } else {
        Ok(a / b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 2), 4);
        assert_eq!(add(-1, 1), 0);
    }

    #[test]
    fn test_divide_success() {
        assert_eq!(divide(10, 2), Ok(5));
    }

    #[test]
    fn test_divide_by_zero() {
        assert_eq!(divide(10, 0), Err("Division by zero"));
    }

    #[test]
    #[should_panic(expected = "overflow")]
    fn test_overflow_panics() {
        // Test that certain input causes panic
        let _ = add(i32::MAX, 1);
    }

    #[test]
    #[ignore]  // Skip by default, run with: cargo test -- --ignored
    fn expensive_test() {
        // Long-running test
    }
}
```

### Integration Tests

```rust
// tests/integration_test.rs
use my_crate::Calculator;

mod common;  // Import shared utilities

#[test]
fn test_full_calculation_flow() {
    let calc = Calculator::new();
    let result = calc.compute("2 + 2 * 3");
    assert_eq!(result, Ok(8));
}
```

```rust
// tests/common/mod.rs
pub fn setup_test_env() -> TestEnv {
    // Shared setup code
}

pub struct TestEnv {
    // Test fixtures
}
```

## Async Tests with Tokio

```toml
[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
```

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_async_function() {
        let result = fetch_data("url").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_with_timeout() {
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            long_running_operation()
        ).await;
        
        assert!(result.is_ok());
    }

    // Test time-dependent code without real delays
    #[tokio::test]
    async fn test_with_paused_time() {
        tokio::time::pause();
        
        let start = tokio::time::Instant::now();
        tokio::time::sleep(Duration::from_secs(100)).await;
        
        // No real time passed!
        assert!(start.elapsed() < Duration::from_millis(10));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_concurrent() {
        // Test with multiple threads
    }
}
```

## Property-Based Testing with Proptest

```toml
[dev-dependencies]
proptest = "1"
```

```rust
use proptest::prelude::*;

fn reverse<T: Clone>(xs: &[T]) -> Vec<T> {
    xs.iter().cloned().rev().collect()
}

proptest! {
    #[test]
    fn test_reverse_twice_is_identity(ref xs in prop::collection::vec(any::<i32>(), 0..100)) {
        let reversed_twice = reverse(&reverse(xs));
        prop_assert_eq!(&reversed_twice, xs);
    }

    #[test]
    fn test_reverse_preserves_length(ref xs in prop::collection::vec(any::<i32>(), 0..100)) {
        prop_assert_eq!(reverse(xs).len(), xs.len());
    }

    #[test]
    fn test_parse_positive_integers(s in "[0-9]{1,5}") {
        let parsed: u32 = s.parse().unwrap();
        prop_assert!(parsed <= 99999);
    }

    // Custom strategies
    #[test]
    fn test_with_custom_strategy(
        x in 1..=100i32,
        y in prop::collection::vec(0..50u8, 1..10)
    ) {
        prop_assert!(x > 0);
        prop_assert!(!y.is_empty());
    }
}

// Custom strategies for complex types
fn valid_email_strategy() -> impl Strategy<Value = String> {
    (
        "[a-z]{1,10}",
        "@",
        "[a-z]{1,10}",
        "\\.(com|org|net)"
    ).prop_map(|(user, at, domain, tld)| {
        format!("{}{}{}{}", user, at, domain, tld)
    })
}

proptest! {
    #[test]
    fn test_email_parsing(email in valid_email_strategy()) {
        assert!(is_valid_email(&email));
    }
}
```

## Mocking

### Using mockall

```toml
[dev-dependencies]
mockall = "0.11"
```

```rust
use mockall::{automock, predicate::*};

#[automock]
trait Database {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str) -> Result<(), Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_mock_database() {
        let mut mock = MockDatabase::new();
        
        // Set expectations
        mock.expect_get()
            .with(eq("user:123"))
            .times(1)
            .returning(|_| Some("John".to_string()));
        
        mock.expect_set()
            .with(eq("user:123"), eq("Jane"))
            .times(1)
            .returning(|_, _| Ok(()));
        
        // Use mock in test
        let service = UserService::new(mock);
        let user = service.get_user("123");
        assert_eq!(user.name, "John");
    }
}
```

### Trait-based dependency injection (no mocking library)

```rust
trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

struct RealClock;
impl Clock for RealClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
struct FakeClock {
    time: DateTime<Utc>,
}

#[cfg(test)]
impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.time
    }
}

struct Service<C: Clock> {
    clock: C,
}

#[test]
fn test_with_fake_clock() {
    let fake_clock = FakeClock { 
        time: Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap() 
    };
    let service = Service { clock: fake_clock };
    // Test time-dependent behavior
}
```

## Test Utilities

### Custom Assertions

```rust
#[cfg(test)]
macro_rules! assert_approx_eq {
    ($left:expr, $right:expr, $epsilon:expr) => {
        let diff = ($left - $right).abs();
        assert!(
            diff < $epsilon,
            "assertion failed: |{} - {}| = {} >= {}",
            $left, $right, diff, $epsilon
        );
    };
}

#[test]
fn test_floating_point() {
    assert_approx_eq!(0.1 + 0.2, 0.3, 1e-10);
}
```

### Test Fixtures with rstest

```toml
[dev-dependencies]
rstest = "0.18"
```

```rust
use rstest::*;

#[fixture]
fn database() -> TestDatabase {
    TestDatabase::new()
}

#[fixture]
fn user(database: TestDatabase) -> User {
    database.create_user("test@example.com")
}

#[rstest]
fn test_user_creation(user: User) {
    assert_eq!(user.email, "test@example.com");
}

// Parameterized tests
#[rstest]
#[case(2, 2, 4)]
#[case(3, 3, 9)]
#[case(0, 5, 0)]
fn test_multiply(#[case] a: i32, #[case] b: i32, #[case] expected: i32) {
    assert_eq!(a * b, expected);
}

// Matrix testing
#[rstest]
fn test_combinations(
    #[values("http", "https")] protocol: &str,
    #[values(80, 443, 8080)] port: u16,
) {
    let url = format!("{}://localhost:{}", protocol, port);
    assert!(url.starts_with(protocol));
}
```

## Snapshot Testing

```toml
[dev-dependencies]
insta = "1"
```

```rust
use insta::assert_snapshot;

#[test]
fn test_render_output() {
    let output = render_template(&data);
    assert_snapshot!(output);
}

#[test]
fn test_json_output() {
    let result = process_data(&input);
    insta::assert_json_snapshot!(result);
}

// Review snapshots: cargo insta review
```

## Test Configuration

### Cargo.toml

```toml
[package]
# ...

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
proptest = "1"
rstest = "0.18"
mockall = "0.11"
insta = "1"
criterion = "0.5"

# Run integration tests in parallel
[[test]]
name = "integration"
path = "tests/integration.rs"

# Separate slow tests
[[test]]
name = "slow_tests"
path = "tests/slow.rs"
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests in a module
cargo test module::tests

# Run ignored tests
cargo test -- --ignored

# Run with output
cargo test -- --nocapture

# Run tests in parallel (default) or single-threaded
cargo test -- --test-threads=1

# Run only doc tests
cargo test --doc

# Run only integration tests
cargo test --test integration
```

## Error Testing

```rust
#[test]
fn test_error_conditions() {
    let result = parse_config("invalid");
    
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    assert!(matches!(err, ConfigError::ParseError { .. }));
    assert!(err.to_string().contains("invalid"));
}

#[test]
fn test_result_ok() {
    let result: Result<i32, &str> = Ok(42);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
}
```

## Test Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html

# With specific features
cargo tarpaulin --features "feature1" --out Lcov
```

## Best Practices

1. **Test behavior, not implementation** - Focus on what functions do, not how
2. **One assertion per test** - When practical, makes failures clear
3. **Descriptive test names** - `test_user_creation_fails_with_invalid_email`
4. **Arrange-Act-Assert pattern** - Clear test structure
5. **Don't test private functions directly** - Test through public API
6. **Use `#[should_panic]` sparingly** - Prefer `Result` for error testing
7. **Keep tests fast** - Mark slow tests with `#[ignore]`
8. **Test edge cases** - Empty inputs, boundaries, error conditions

## References

- [Rust Book - Testing](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Proptest Book](https://proptest-rs.github.io/proptest/intro.html)
- [mockall Documentation](https://docs.rs/mockall)
- [rstest Documentation](https://docs.rs/rstest)
