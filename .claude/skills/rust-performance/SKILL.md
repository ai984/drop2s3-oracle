---
name: rust-performance
description: Rust performance optimization, profiling with flamegraph/perf/samply, benchmarking with criterion/divan, memory optimization, and build configuration tuning. Use when optimizing Rust code, analyzing bottlenecks, or configuring release builds. Triggers on performance issues, slow code, profiling requests, or Cargo.toml optimization.
---

# Rust Performance Optimization Guide

You are an expert in Rust performance optimization, profiling, and benchmarking.

## Profiling Tools

### Flamegraph (Recommended for quick analysis)

```bash
# Install
cargo install flamegraph

# Generate flamegraph (requires perf on Linux)
cargo flamegraph --bin my-app

# With specific features
cargo flamegraph --features "feature1,feature2"
```

### Samply (Cross-platform, Firefox Profiler integration)

```bash
# Install
cargo install samply

# Profile
samply record ./target/release/my-app

# Opens Firefox Profiler automatically
```

### Perf (Linux - most detailed)

```bash
# Record
perf record -g --call-graph dwarf ./target/release/my-app

# Report
perf report

# Flame graph from perf data
perf script | inferno-collapse-perf | inferno-flamegraph > flamegraph.svg
```

### Cachegrind (Instruction-level analysis)

```bash
valgrind --tool=cachegrind ./target/release/my-app
cg_annotate cachegrind.out.*
```

## Benchmarking

### Criterion (Statistical benchmarking)

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "my_benchmark"
harness = false
```

```rust
// benches/my_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fibonacci(n-1) + fibonacci(n-2),
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("fib 20", |b| b.iter(|| fibonacci(black_box(20))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
```

```bash
cargo bench
```

### Divan (Simpler alternative)

```toml
[dev-dependencies]
divan = "0.1"

[[bench]]
name = "example"
harness = false
```

```rust
fn main() {
    divan::main();
}

#[divan::bench]
fn fibonacci() -> u64 {
    // Function to benchmark
}
```

## Build Configuration (Cargo.toml)

### Maximum Runtime Speed

```toml
[profile.release]
codegen-units = 1      # Better optimization, slower compile
lto = "fat"            # Link-time optimization (10-20% faster)
panic = "abort"        # Smaller binary, slightly faster
strip = "symbols"      # Smaller binary

[profile.release.build-override]
opt-level = 3
```

### CPU-Specific Optimizations

```bash
# Use native CPU instructions (not portable!)
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

Or in `.cargo/config.toml`:

```toml
[build]
rustflags = ["-C", "target-cpu=native"]
```

### Profile-Guided Optimization (PGO)

```bash
# Install cargo-pgo
cargo install cargo-pgo

# Build instrumented binary
cargo pgo build

# Run with representative workload
./target/x86_64-unknown-linux-gnu/release/my-app

# Build optimized binary
cargo pgo optimize
```

## Alternative Allocators

### jemalloc (Linux/Mac - often faster)

```toml
[dependencies]
tikv-jemallocator = "0.5"
```

```rust
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
```

### mimalloc (Cross-platform)

```toml
[dependencies]
mimalloc = "0.1"
```

```rust
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```

## Memory Optimization

### Arena Allocation (bumpalo)

```toml
[dependencies]
bumpalo = "3"
```

```rust
use bumpalo::Bump;

fn process_batch(data: &[Item]) {
    let arena = Bump::new();
    
    // Fast allocations - all freed at once when arena drops
    for item in data {
        let processed = arena.alloc(process(item));
        // use processed...
    }
    // All memory freed here automatically
}
```

### Zero-Copy Patterns

```rust
use std::borrow::Cow;

// Accept either borrowed or owned data
fn process(data: Cow<'_, str>) -> String {
    if needs_modification(&data) {
        data.into_owned()  // Clone only if needed
    } else {
        data.into_owned()
    }
}

// Use slices instead of Vec when possible
fn analyze(data: &[u8]) -> Analysis {
    // No allocation, works with any contiguous memory
}
```

## Common Optimizations

### Avoid Unnecessary Allocations

```rust
// BAD: Multiple allocations
let filtered: Vec<_> = data.iter().filter(|x| x.is_valid()).collect();
let mapped: Vec<_> = filtered.iter().map(|x| x.transform()).collect();

// GOOD: Single pass, lazy evaluation
let result: Vec<_> = data.iter()
    .filter(|x| x.is_valid())
    .map(|x| x.transform())
    .collect();
```

### Use `with_capacity` for known sizes

```rust
// BAD: Multiple reallocations
let mut vec = Vec::new();
for i in 0..1000 {
    vec.push(i);
}

// GOOD: Single allocation
let mut vec = Vec::with_capacity(1000);
for i in 0..1000 {
    vec.push(i);
}
```

### Prefer stack over heap

```rust
// Heap allocation
let data = Box::new([0u8; 1024]);

// Stack allocation (faster, if size is known and reasonable)
let data = [0u8; 1024];
```

### Use `SmallVec` for small collections

```toml
[dependencies]
smallvec = "1"
```

```rust
use smallvec::SmallVec;

// Stores up to 8 elements on stack, heap only if more needed
let mut vec: SmallVec<[i32; 8]> = SmallVec::new();
```

## Faster Hashing

### Use FxHash for HashMaps (not cryptographic!)

```toml
[dependencies]
rustc-hash = "1"
```

```rust
use rustc_hash::FxHashMap;

// 2-3x faster than default HashMap for small keys
let mut map: FxHashMap<u32, String> = FxHashMap::default();
```

### Use AHash (good balance)

```toml
[dependencies]
ahash = "0.8"
```

```rust
use ahash::AHashMap;
let mut map: AHashMap<String, i32> = AHashMap::new();
```

## Compile Time Optimization

### Faster Linking (Linux)

```toml
# .cargo/config.toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

Install mold: `sudo apt install mold` or `brew install mold`

### Disable Debug Info in Dev

```toml
[profile.dev]
debug = false  # 20-40% faster compiles
```

## Debug Info for Profiling

```toml
[profile.release]
debug = "line-tables-only"  # Enables source-level profiling
```

## Inlining

```rust
// Suggest inlining for small, hot functions
#[inline]
fn small_hot_function() { }

// Force inlining (use sparingly)
#[inline(always)]
fn critical_hot_path() { }

// Prevent inlining for cold paths
#[inline(never)]
fn error_handling_cold_path() { }

// Mark cold functions
#[cold]
fn rarely_called() { }
```

## References

- [The Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Criterion Documentation](https://bheisler.github.io/criterion.rs/book/)
- [Flamegraph](https://github.com/flamegraph-rs/flamegraph)
- [cargo-pgo](https://github.com/Kobzol/cargo-pgo)
- [min-sized-rust](https://github.com/johnthagen/min-sized-rust)
