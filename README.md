# apple-main

Seamlessly integrate async Rust (tokio) with Apple's main-thread-bound frameworks like Virtualization.framework and AppKit.

## Cross-Platform by Design

**Write once, run everywhere.** All `apple-main` APIs work transparently on non-Apple platforms:

| API | macOS | Linux/Windows |
|-----|-------|---------------|
| `on_main()` | Dispatches to main thread via GCD | Executes inline |
| `on_main_sync()` | Blocks until main thread completes | Executes inline |
| `is_main_thread()` | Checks via pthread | Always returns `true` |
| `#[apple_main::main]` | Runs CFRunLoop on main | Standard `#[tokio::main]` |

This means your cross-platform code "just works" everywhere without conditional compilation.

## The Problem

Apple frameworks often require:
- **Main thread execution**: Certain APIs must be called from the main thread
- **CFRunLoop**: Some frameworks need a running `CFRunLoop` on the main thread
- **Code signing**: Binaries must be signed with specific entitlements

These requirements conflict with `#[tokio::main]`, which takes ownership of the main thread for async execution.

## ⚠️ Critical: Why `#[tokio::main]` Doesn't Work

**`#[tokio::main]` will cause `on_main()` to hang on macOS.** Here's why:

```rust
#[tokio::main]  // ❌ DON'T DO THIS for Apple framework code
async fn main() {
    // This will HANG forever on macOS!
    let config = on_main(|| VZVirtualMachineConfiguration::new()).await;
}
```

The `on_main()` function dispatches work to the main dispatch queue via GCD. But someone needs to **drain that queue** - typically CFRunLoop. With `#[tokio::main]`, tokio owns the main thread and the main queue never gets drained.

**Always use `#[apple_main::main]` when your code uses `on_main()`:**

```rust
#[apple_main::main]  // ✅ CORRECT
async fn main() {
    // This works! CFRunLoop runs on main thread, draining the queue
    let config = on_main(|| VZVirtualMachineConfiguration::new()).await;
}
```

## Solution

```rust
#[apple_main::main]
async fn main() {
    // Your async code runs on tokio worker threads
    // Main thread runs CFRunLoop, available for Apple APIs via on_main()

    let config = apple_main::on_main(|| {
        VZVirtualMachineConfiguration::new()
    }).await;

    println!("VM configured!");
}
```

On macOS, this:
1. Spawns your async code on the tokio runtime (background threads)
2. Runs CFRunLoop on the main thread
3. `on_main()` dispatches closures to the main thread and awaits completion

On other platforms, `#[apple_main::main]` is equivalent to `#[tokio::main]`.

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
| Any code using `on_main()` | `#[apple_main::main]` |
| Headless VM server | `#[apple_main::main]` |
| CLI tool managing VMs | `#[apple_main::main]` |
| Desktop app with GUI | `#[apple_main::main]` |
| Code that doesn't need main thread | `#[tokio::main]` (standard tokio) |

**Rule of thumb:** If your code touches Apple frameworks that require the main thread (Virtualization.framework, AppKit, etc.), use `#[apple_main::main]`.

## Before & After: Cross-Platform VM Code

### Without apple-main (platform-specific)

```rust
// Requires #[cfg] everywhere and different code paths per platform
#[cfg(target_os = "macos")]
mod vm {
    use dispatch::Queue;
    use std::sync::mpsc;

    pub fn create_vm_config() -> VZVirtualMachineConfiguration {
        let (tx, rx) = mpsc::channel();
        Queue::main().exec_async(move || {
            let config = VZVirtualMachineConfiguration::new();
            tx.send(config).unwrap();
        });
        rx.recv().unwrap()
    }
}

#[cfg(not(target_os = "macos"))]
mod vm {
    pub fn create_vm_config() -> MockConfig {
        MockConfig::new()  // Different API!
    }
}
```

### With apple-main (unified)

```rust
// Same code works on all platforms
use apple_main::on_main;

async fn create_vm_config() -> VZVirtualMachineConfiguration {
    on_main(|| VZVirtualMachineConfiguration::new()).await
}
```

On macOS, this dispatches to the main thread via GCD. On other platforms, it executes inline. No conditional compilation needed.

## Testing

### ⚠️ `#[tokio::test]` Won't Work

Just like `#[tokio::main]`, **`#[tokio::test]` will hang** when using `on_main()`:

```rust
#[tokio::test]  // ❌ Will HANG on macOS!
async fn test_vm_creation() {
    let config = apple_main::on_main(|| {
        VZVirtualMachineConfiguration::new()
    }).await;
}
```

### Solution: Custom Test Harness

For tests that use `on_main()`, use `#[apple_main::harness_test]` with `harness = false`:

```toml
# Cargo.toml
[[test]]
name = "vm_tests"
harness = false
```

```rust
// tests/vm_tests.rs
#[apple_main::harness_test]
async fn test_vm_creation() {
    let config = apple_main::on_main(|| {
        VZVirtualMachineConfiguration::new()
    }).await;
    assert!(config.is_valid());
}

#[apple_main::harness_test]
async fn test_vm_boot() {
    // Another test
}

apple_main::test_main!();
```

The `test_main!()` macro:
1. Runs CFRunLoop on the main thread (so `on_main()` works)
2. Executes tests on the tokio runtime
3. Uses `libtest-mimic` for standard test output (`--filter`, `--nocapture`, etc.)

### Tests That Don't Need Main Thread

For code that doesn't use `on_main()`, standard `#[tokio::test]` works fine:

```rust
#[tokio::test]
async fn test_config_parsing() {
    // No on_main() here - just regular async code
    let config = parse_config("test.yaml").await;
    assert!(config.is_ok());
}
```

## Benchmarking

Benchmarks need CFRunLoop running on the main thread, just like tests and applications.

### With Criterion

Enable the `criterion` feature and use `apple_main::criterion_main!` instead of Criterion's `criterion_main!`:

```toml
# Cargo.toml
[dev-dependencies]
apple-main = { version = "0.1", features = ["criterion"] }

[[bench]]
name = "vm_bench"
harness = false
```

```rust
// benches/vm_bench.rs
use apple_main::criterion::{criterion_group, Criterion};

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
apple_main::criterion_main!(benches);  // Use apple_main's version!
```

Run benchmarks (automatically signed via codesign-run):

```bash
cargo bench
```

The `apple_main::criterion_main!` macro handles the CFRunLoop setup automatically:
- On macOS: Runs CFRunLoop on main thread, Criterion on background thread
- On other platforms: Runs Criterion normally

### With Divan (Manual Setup)

Divan doesn't have built-in apple-main support yet, so manual setup is required:

```toml
[dev-dependencies]
divan = "0.1"

[target.'cfg(target_os = "macos")'.dev-dependencies]
dispatch = "0.2"
core-foundation = "0.10"

[[bench]]
name = "vm_bench"
harness = false
```

```rust
// benches/vm_bench.rs

fn main() {
    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            divan::main();
            dispatch::Queue::main().exec_async(|| {
                core_foundation::runloop::CFRunLoop::get_current().stop();
            });
        });
        core_foundation::runloop::CFRunLoop::run_current();
    }

    #[cfg(not(target_os = "macos"))]
    divan::main();
}

#[divan::bench]
fn vm_create() {
    apple_main::on_main_sync(|| {
        VZVirtualMachineConfiguration::new()
    });
}
```

## Library Integration

If you're building a library (like a VM abstraction layer) that supports multiple backends including Apple's Virtualization.framework, here's the recommended pattern:

### Expose Async Traits

```rust
#[async_trait]
pub trait VmBackend: Send + Sync {
    async fn create_vm(&self, config: &VmConfig) -> Result<Box<dyn VmHandle>>;
}
```

### Implement with `on_main()` for Apple Backend

```rust
pub struct AppleVirtualizationBackend;

#[async_trait]
impl VmBackend for AppleVirtualizationBackend {
    async fn create_vm(&self, config: &VmConfig) -> Result<Box<dyn VmHandle>> {
        // Dispatch the Apple API calls to main thread
        let vm = apple_main::on_main(move || {
            let vz_config = VZVirtualMachineConfiguration::new();
            vz_config.set_cpu_count(config.cpus);
            vz_config.set_memory_size(config.memory);
            VZVirtualMachine::new(vz_config)
        }).await;

        Ok(Box::new(AppleVmHandle::new(vm)))
    }
}
```

### Document the Runtime Requirement

Your library's README should note:

> **macOS with native backend requires `#[apple_main::main]`**
>
> ```rust
> #[apple_main::main]  // Required for native Virtualization.framework
> async fn main() {
>     let backend = select_backend();  // Returns AppleVirtualizationBackend on macOS
>     let vm = backend.create_vm(&config).await?;
> }
> ```

### Cross-Platform Considerations

The beauty of this pattern is that `on_main()` is a no-op on non-Apple platforms:

```rust
// This same code works everywhere:
// - macOS: dispatches to main thread
// - Linux/Windows: executes inline

let vm = apple_main::on_main(|| create_vm()).await;
```

Your library consumers write the same code regardless of platform. The only difference is using `#[apple_main::main]` instead of `#[tokio::main]` - which is also a no-op on non-Apple platforms.

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
