---
name: rust-unsafe-ffi
description: Guidelines for writing safe unsafe Rust code and FFI (Foreign Function Interface) bindings. Covers raw pointers, memory safety, C interop, and common pitfalls. Use when writing unsafe blocks, creating FFI bindings, or interfacing with C/C++ libraries. Triggers on unsafe keyword, FFI, extern "C", raw pointers, or C interop questions.
---

# Unsafe Rust and FFI Best Practices

You are an expert in unsafe Rust code and Foreign Function Interface (FFI) bindings.

## When to Use Unsafe

Unsafe Rust is necessary for:
1. Dereferencing raw pointers
2. Calling unsafe functions (including FFI)
3. Accessing/modifying mutable static variables
4. Implementing unsafe traits
5. Accessing union fields

## Unsafe Code Guidelines

### Minimize Unsafe Scope

```rust
// BAD: Large unsafe block
unsafe {
    let ptr = get_raw_pointer();
    let len = calculate_length();  // Safe operation in unsafe block
    let data = std::slice::from_raw_parts(ptr, len);
    process_data(data);  // Safe operation in unsafe block
}

// GOOD: Minimal unsafe scope
let ptr = get_raw_pointer();
let len = calculate_length();
let data = unsafe { std::slice::from_raw_parts(ptr, len) };
process_data(data);
```

### Document Safety Requirements

```rust
/// Creates a slice from raw parts.
///
/// # Safety
///
/// - `ptr` must be valid for reads for `len * size_of::<T>()` bytes
/// - `ptr` must be properly aligned
/// - `ptr` must point to `len` consecutive properly initialized values of type `T`
/// - The memory must not be mutated while the slice exists (except inside `UnsafeCell`)
/// - The total size must not exceed `isize::MAX` bytes
pub unsafe fn from_raw_parts<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    // ...
}
```

### Create Safe Abstractions

```rust
pub struct Buffer {
    ptr: *mut u8,
    len: usize,
    capacity: usize,
}

impl Buffer {
    /// Creates a new buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let layout = std::alloc::Layout::array::<u8>(capacity).unwrap();
        // SAFETY: layout has non-zero size
        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Self { ptr, len: 0, capacity }
    }

    /// Returns a slice of the buffer's contents.
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: ptr is valid for len bytes, properly aligned, and initialized
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// Appends a byte to the buffer.
    pub fn push(&mut self, byte: u8) {
        if self.len >= self.capacity {
            self.grow();
        }
        // SAFETY: we just ensured capacity > len
        unsafe {
            self.ptr.add(self.len).write(byte);
        }
        self.len += 1;
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let layout = std::alloc::Layout::array::<u8>(self.capacity).unwrap();
        // SAFETY: ptr was allocated with this layout
        unsafe { std::alloc::dealloc(self.ptr, layout) };
    }
}
```

## FFI Basics

### Declaring External Functions

```rust
// Link to C library
#[link(name = "mylib")]
extern "C" {
    fn c_function(x: i32) -> i32;
    fn c_string_function(s: *const std::ffi::c_char) -> *mut std::ffi::c_char;
}

// Call the function
fn call_c() {
    let result = unsafe { c_function(42) };
}
```

### Type Mappings (Rust ↔ C)

| Rust | C |
|------|---|
| `i8` | `int8_t` / `char` |
| `i16` | `int16_t` / `short` |
| `i32` | `int32_t` / `int` |
| `i64` | `int64_t` / `long long` |
| `isize` | `intptr_t` / `ssize_t` |
| `u8` | `uint8_t` / `unsigned char` |
| `u16` | `uint16_t` / `unsigned short` |
| `u32` | `uint32_t` / `unsigned int` |
| `u64` | `uint64_t` / `unsigned long long` |
| `usize` | `size_t` / `uintptr_t` |
| `f32` | `float` |
| `f64` | `double` |
| `bool` | `bool` (C99) / `_Bool` |
| `()` | `void` (return type only) |
| `*const T` | `const T*` |
| `*mut T` | `T*` |
| `&T` | `const T*` (with restrictions) |
| `&mut T` | `T*` (with restrictions) |

### Use std::ffi types

```rust
use std::ffi::{c_char, c_int, c_void, CStr, CString};

extern "C" {
    fn strlen(s: *const c_char) -> usize;
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
}
```

## String Handling

### Passing Strings to C

```rust
use std::ffi::CString;

fn call_c_with_string(s: &str) -> Result<(), std::ffi::NulError> {
    let c_string = CString::new(s)?;  // Adds null terminator
    unsafe {
        c_function(c_string.as_ptr());
    }
    Ok(())
}

// DANGER: This is wrong!
fn wrong_way(s: &str) {
    let c_string = CString::new(s).unwrap();
    let ptr = c_string.as_ptr();
    // c_string is dropped here!
    unsafe { c_function(ptr) };  // Use-after-free!
}

// Correct way
fn correct_way(s: &str) {
    let c_string = CString::new(s).unwrap();
    unsafe { c_function(c_string.as_ptr()) };  // c_string still alive
}
```

### Receiving Strings from C

```rust
use std::ffi::CStr;

unsafe fn get_string_from_c() -> String {
    let ptr = c_get_string();  // Returns *const c_char
    
    // Option 1: Borrowed (no copy, but lifetime tied to C memory)
    let c_str = CStr::from_ptr(ptr);
    let rust_str = c_str.to_str().unwrap();  // Returns &str
    
    // Option 2: Owned (copies the string)
    let owned = c_str.to_string_lossy().into_owned();
    
    owned
}
```

## Structs and Repr

### C-compatible structs

```rust
// Ensure C-compatible memory layout
#[repr(C)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

// Packed struct (no padding)
#[repr(C, packed)]
pub struct PackedData {
    pub flag: u8,
    pub value: u32,
}

// Specify alignment
#[repr(C, align(16))]
pub struct AlignedData {
    pub data: [u8; 32],
}
```

### Opaque Types

```rust
// When C uses opaque pointers
#[repr(C)]
pub struct OpaqueHandle {
    _private: [u8; 0],
}

extern "C" {
    fn create_handle() -> *mut OpaqueHandle;
    fn destroy_handle(handle: *mut OpaqueHandle);
    fn use_handle(handle: *mut OpaqueHandle) -> i32;
}
```

## Callbacks

### Rust function as C callback

```rust
// Define callback type
type CCallback = extern "C" fn(i32) -> i32;

extern "C" {
    fn register_callback(cb: CCallback);
}

// Implement callback
extern "C" fn my_callback(x: i32) -> i32 {
    x * 2
}

fn setup() {
    unsafe { register_callback(my_callback) };
}
```

### Callbacks with user data

```rust
type CCallbackWithData = extern "C" fn(*mut c_void, i32) -> i32;

extern "C" {
    fn register_callback_with_data(cb: CCallbackWithData, user_data: *mut c_void);
}

struct Context {
    multiplier: i32,
}

extern "C" fn callback_with_context(user_data: *mut c_void, x: i32) -> i32 {
    let context = unsafe { &*(user_data as *const Context) };
    x * context.multiplier
}

fn setup_with_context() {
    let context = Box::new(Context { multiplier: 3 });
    let context_ptr = Box::into_raw(context) as *mut c_void;
    
    unsafe { register_callback_with_data(callback_with_context, context_ptr) };
    
    // Remember to free context when done!
    // unsafe { drop(Box::from_raw(context_ptr as *mut Context)) };
}
```

## Error Handling

### Returning errors from Rust to C

```rust
#[repr(C)]
pub enum ErrorCode {
    Success = 0,
    InvalidInput = 1,
    OutOfMemory = 2,
    IoError = 3,
}

#[no_mangle]
pub extern "C" fn process_data(ptr: *const u8, len: usize) -> ErrorCode {
    if ptr.is_null() {
        return ErrorCode::InvalidInput;
    }
    
    let data = unsafe { std::slice::from_raw_parts(ptr, len) };
    
    match do_processing(data) {
        Ok(_) => ErrorCode::Success,
        Err(e) => match e {
            ProcessError::InvalidInput => ErrorCode::InvalidInput,
            ProcessError::IoError(_) => ErrorCode::IoError,
        }
    }
}
```

## Common Pitfalls

### 1. Null Pointer Dereference

```rust
// BAD
unsafe fn dangerous(ptr: *const i32) -> i32 {
    *ptr  // May be null!
}

// GOOD
unsafe fn safe(ptr: *const i32) -> Option<i32> {
    if ptr.is_null() {
        None
    } else {
        Some(*ptr)
    }
}
```

### 2. Lifetime/Ownership Confusion

```rust
// BAD: Returning pointer to local variable
#[no_mangle]
pub extern "C" fn bad_return() -> *const u8 {
    let data = vec![1, 2, 3];
    data.as_ptr()  // data is dropped, pointer is dangling!
}

// GOOD: Transfer ownership
#[no_mangle]
pub extern "C" fn good_return(out_len: *mut usize) -> *mut u8 {
    let mut data = vec![1, 2, 3];
    let ptr = data.as_mut_ptr();
    let len = data.len();
    
    unsafe { *out_len = len };
    std::mem::forget(data);  // Don't drop, C owns it now
    
    ptr
}

// Don't forget to provide a free function!
#[no_mangle]
pub extern "C" fn free_buffer(ptr: *mut u8, len: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, len, len);
        // Vec is dropped here, memory freed
    }
}
```

### 3. Alignment Issues

```rust
// BAD: May cause undefined behavior on some platforms
unsafe fn misaligned_read(ptr: *const u8) -> u32 {
    *(ptr as *const u32)  // ptr may not be aligned for u32!
}

// GOOD: Handle unaligned reads explicitly
unsafe fn aligned_read(ptr: *const u8) -> u32 {
    std::ptr::read_unaligned(ptr as *const u32)
}
```

## Using bindgen

For automatic FFI binding generation:

```toml
[build-dependencies]
bindgen = "0.69"
```

```rust
// build.rs
use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-link-lib=mylib");
    
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");
    
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
```

## Miri for Undefined Behavior Detection

```bash
# Install miri
rustup +nightly component add miri

# Run tests with miri
cargo +nightly miri test
```

## References

- [The Rustonomicon](https://doc.rust-lang.org/nomicon/)
- [Rust FFI Omnibus](http://jakegoulding.com/rust-ffi-omnibus/)
- [Unsafe Code Guidelines](https://rust-lang.github.io/unsafe-code-guidelines/)
- [bindgen User Guide](https://rust-lang.github.io/rust-bindgen/)
