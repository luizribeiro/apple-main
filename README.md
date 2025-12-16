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
