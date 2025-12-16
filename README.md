# apple-main

Seamlessly integrate async Rust (tokio) with Apple's main-thread-bound frameworks like Virtualization.framework and AppKit.

## The Problem

Apple frameworks often require:
- **Main thread execution**: Certain APIs must be called from the main thread
- **CFRunLoop**: Some frameworks need a running `CFRunLoop` on the main thread
- **Code signing**: Binaries must be signed with specific entitlements

These requirements conflict with `#[tokio::main]`, which takes ownership of the main thread for async execution.

## Solution

`apple-main` provides two approaches:

### Option 1: Dispatch Helpers (Recommended for headless use)

Use standard `#[tokio::main]` and dispatch specific calls to the main thread:

```rust
use apple_main::on_main;

#[tokio::main]
async fn main() {
    // This closure runs on the main thread via GCD
    let config = on_main(|| {
        VZVirtualMachineConfiguration::new()
    }).await;

    // Back on tokio worker threads
    println!("VM configured!");
}
```

### Option 2: Main Thread Macro (For GUI or CFRunLoop-dependent code)

When you need the main thread to run CFRunLoop:

```rust
use apple_main::main;

#[apple_main::main]
async fn main() {
    // Your async code runs on tokio
    // Main thread is available for Apple APIs via on_main()
}
```

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
apple-main = "0.1"
```

### Code Signing Setup

Install the codesign runner:

```bash
cargo install --path codesign-run
```

Configure Cargo to automatically sign binaries:

```toml
# .cargo/config.toml
[target.aarch64-apple-darwin]
runner = "codesign-run"

[target.x86_64-apple-darwin]
runner = "codesign-run"
```

Or with custom entitlements:

```toml
runner = ["codesign-run", "--entitlements", "path/to/entitlements.xml"]
```

## API

### Dispatch Helpers

```rust
// Async - dispatch to main thread and await result
let result = apple_main::on_main(|| {
    // Runs on main thread
    create_vm_config()
}).await;

// Sync - block until main thread completes
let result = apple_main::on_main_sync(|| {
    // Runs on main thread
    create_vm_config()
});
```

### Runtime Management

```rust
// Initialize the shared tokio runtime
let rt = apple_main::init_runtime();

// Get the runtime (panics if not initialized)
let rt = apple_main::runtime();

// Block on a future using the shared runtime
let result = apple_main::block_on(async { 42 });
```

### Platform Detection

```rust
if apple_main::is_main_thread() {
    // Safe to call main-thread-only APIs
}
```

## Cross-Platform Support

All APIs work on non-Apple platforms as transparent passthroughs:
- `on_main()` / `on_main_sync()` execute inline
- `is_main_thread()` always returns `true`
- `#[apple_main::main]` expands to `#[tokio::main]`

## Entitlements

The crate includes common entitlement files in `entitlements/`:

- `virtualization.entitlements` - For Virtualization.framework
- `hypervisor.entitlements` - For Hypervisor.framework
- `combined.entitlements` - All entitlements including VM networking

## When to Use What

| Use Case | Approach |
|----------|----------|
| Headless VM server | `#[tokio::main]` + `on_main()` |
| CLI tool managing VMs | `#[tokio::main]` + `on_main()` |
| gRPC/HTTP service | `#[tokio::main]` + `on_main()` |
| Desktop app with GUI | `#[apple_main::main]` |
| VZVirtualMachineView | `#[apple_main::main]` |

## Testing

### Option 1: Standard tokio tests (Recommended)

For most cases, use `#[tokio::test]` with `on_main()`:

```rust
#[tokio::test]
async fn test_vm_creation() {
    let config = apple_main::on_main(|| {
        VZVirtualMachineConfiguration::new()
    }).await;

    assert!(config.is_valid());
}
```

### Option 2: apple_main::test macro

For tests that need the shared runtime:

```rust
#[apple_main::test]
async fn test_with_shared_runtime() {
    // Uses apple_main::init_runtime() and block_on()
    let result = some_async_operation().await;
    assert!(result.is_ok());
}
```

### Important: Main Thread Limitations in Tests

Cargo's test harness runs tests on worker threads, not the main thread. This means:

- `on_main_sync()` will **hang** on macOS in tests (no active main dispatch queue)
- `on_main()` async version also requires the main queue to be drained

**Workaround for macOS tests that need main thread:**

For code that truly requires the main thread with an active runloop, you need
integration tests with `harness = false` that set up their own main loop:

```toml
# Cargo.toml
[[test]]
name = "macos_integration"
harness = false
```

```rust
// tests/macos_integration.rs
fn main() {
    // Set up your own test runner with CFRunLoop
    // This is an advanced use case
}
```

## Benchmarking

### With Criterion (Recommended)

Criterion works seamlessly with the `codesign-run` runner:

```toml
# Cargo.toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "vm_bench"
harness = false
```

```rust
// benches/vm_bench.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn vm_benchmark(c: &mut Criterion) {
    c.bench_function("vm_create", |b| {
        b.iter(|| {
            apple_main::on_main_sync(|| {
                VZVirtualMachineConfiguration::new()
            })
        })
    });
}

criterion_group!(benches, vm_benchmark);
criterion_main!(benches);
```

Run benchmarks (automatically signed via codesign-run):

```bash
cargo bench
```

### With Divan

```toml
[dev-dependencies]
divan = "0.1"

[[bench]]
name = "vm_bench"
harness = false
```

```rust
// benches/vm_bench.rs
use divan::Bencher;

#[divan::bench]
fn vm_create(bencher: Bencher) {
    bencher.bench(|| {
        apple_main::on_main_sync(|| {
            VZVirtualMachineConfiguration::new()
        })
    });
}

fn main() {
    divan::main();
}
```

## Not Yet Implemented

The following features from the design are not yet implemented:

- `#[apple_main::test_module]` - Module-level test attribute
- `apple_main::test_main!()` - Custom test harness macro
- `inventory` + `libtest-mimic` based custom test harness for CFRunLoop-dependent tests

For now, tests requiring an active main runloop need manual setup with `harness = false`.

## License

MIT License

Copyright (c) 2024

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
