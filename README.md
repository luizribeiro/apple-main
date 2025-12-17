# apple-main

Seamlessly integrate async Rust (tokio) with Apple's main-thread-bound frameworks like Virtualization.framework and AppKit.

## The Problem

Apple frameworks often require:
- **Main thread execution**: Certain APIs must be called from the main thread
- **CFRunLoop**: The main dispatch queue needs an active runloop to process work

This conflicts with `#[tokio::main]`, which blocks the main thread running the async runtime. Work dispatched to the main queue via GCD never gets processed:

```rust
#[tokio::main]
async fn main() {
    // Dispatch work to main queue via GCD
    dispatch::Queue::main().exec_async(|| {
        println!("This never runs!");  // Main queue is never drained
    });

    tokio::time::sleep(Duration::from_secs(10)).await;
}
```

The same problem affects `#[tokio::test]` - tests that need the main thread will hang.

## Solution

`apple-main` provides `#[apple_main::main]` which runs CFRunLoop on the main thread while your async code runs on tokio:

```text
┌─────────────────┐     dispatch      ┌─────────────────┐
│  Tokio Thread   │ ───────────────▶  │   Main Thread   │
│  (your code)    │                   │   (CFRunLoop)   │
│                 │ ◀───────────────  │                 │
└─────────────────┘     result        └─────────────────┘
```

```rust
#[apple_main::main]
async fn main() {
    // Your async code runs on tokio (background threads)
    // Main thread runs CFRunLoop, available via on_main()

    let config = apple_main::on_main(|| {
        // This runs on the main thread
        VZVirtualMachineConfiguration::new()
    }).await;

    println!("VM configured!");
}
```

**On non-macOS platforms**, `#[apple_main::main]` expands to `#[tokio::main]`, so you can use the same code everywhere without conditional compilation.

## Installation

```toml
[dependencies]
apple-main = "0.1"
```

### Code Signing (macOS)

Apple frameworks like Virtualization.framework require signed binaries with specific entitlements. Install the `codesign-run` helper:

```bash
cargo install --path codesign-run
```

Then configure Cargo to sign binaries automatically:

```toml
# .cargo/config.toml
[target.aarch64-apple-darwin]
runner = "codesign-run"

[target.x86_64-apple-darwin]
runner = "codesign-run"
```

Now `cargo run`, `cargo test`, and `cargo bench` automatically sign before execution.

For custom entitlements:

```toml
runner = ["codesign-run", "--entitlements", "path/to/entitlements.plist"]
```

The crate includes common entitlement files in `entitlements/`:
- `virtualization.entitlements` - For Virtualization.framework
- `hypervisor.entitlements` - For Hypervisor.framework
- `combined.entitlements` - All entitlements including VM networking

## API

### Main Thread Dispatch

```rust
// Async - dispatch to main thread and await result
let result = apple_main::on_main(|| {
    // Runs on main thread
    VZVirtualMachineConfiguration::new()
}).await;

// Sync - block current thread until main thread completes
let result = apple_main::on_main_sync(|| {
    // Runs on main thread
    VZVirtualMachineConfiguration::new()
});
```

### Thread Detection

```rust
if apple_main::is_main_thread() {
    // We're on the main thread
}
```

## Before & After

### Without apple-main

```rust
use std::sync::mpsc;

fn main() {
    // Manually coordinate threads and runloop
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Need to manually dispatch to main queue
            let (result_tx, result_rx) = mpsc::channel();
            dispatch::Queue::main().exec_async(move || {
                let config = VZVirtualMachineConfiguration::new();
                result_tx.send(config).unwrap();
            });
            let config = result_rx.recv().unwrap();

            // Continue with config...
        });
        tx.send(()).unwrap();
    });

    // Main thread must run the runloop
    loop {
        core_foundation::runloop::CFRunLoop::run_current();
        if rx.try_recv().is_ok() { break; }
    }
}
```

### With apple-main

```rust
#[apple_main::main]
async fn main() {
    let config = apple_main::on_main(|| {
        VZVirtualMachineConfiguration::new()
    }).await;

    // Continue with config...
}
```

## Testing

Use `#[apple_main::harness_test]` for tests that need `on_main()`:

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
    assert!(config.validate().is_ok());
}

#[apple_main::harness_test]
async fn test_vm_boot() {
    // ...
}

apple_main::test_main!();
```

The test harness:
- Runs CFRunLoop on the main thread (so `on_main()` works)
- Executes tests on the tokio runtime
- Uses `libtest-mimic` for standard test output (`--filter`, `--nocapture`, etc.)

**On non-macOS platforms**, the harness runs tests normally without CFRunLoop overhead.

### Tests That Don't Need Main Thread

For tests that don't use `on_main()`, standard `#[tokio::test]` works:

```rust
#[tokio::test]
async fn test_config_parsing() {
    let config = parse_config("test.yaml").await;
    assert!(config.is_ok());
}
```

## Benchmarking with Criterion

Enable the `criterion` feature for easy Criterion integration:

```toml
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
apple_main::criterion_main!(benches);  // Use apple_main's version
```

The `apple_main::criterion_main!` macro handles CFRunLoop setup automatically.

## Library Integration

If you're building a library with multiple backends (e.g., QEMU, Virtualization.framework), use `on_main()` in the Apple backend implementation.

`on_main()` returns a `Send` future, so it works across `async_trait` boundaries:

```rust
#[async_trait]
impl VmBackend for AppleVirtualizationBackend {
    async fn create_vm(&self, config: &VmConfig) -> Result<Box<dyn VmHandle>> {
        let vm = apple_main::on_main(move || {
            let vz_config = VZVirtualMachineConfiguration::new();
            vz_config.set_cpu_count(config.cpus);
            VZVirtualMachine::new(vz_config)
        }).await;

        Ok(Box::new(AppleVmHandle::new(vm)))
    }
}
```

Document that consumers using this backend need `#[apple_main::main]`:

```rust
#[apple_main::main]  // Required for Virtualization.framework backend
async fn main() {
    let vm = backend.create_vm(&config).await?;
}
```

## License

MIT License - see [LICENSE](LICENSE) for details.
