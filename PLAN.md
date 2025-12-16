# apple-main

A Rust crate for seamlessly integrating async Rust (tokio) with Apple's main-thread-bound frameworks (Virtualization.framework, AppKit, etc.) while maintaining cross-platform compatibility.

## Problem Statement

Apple frameworks like `Virtualization.framework` require:

1. **Main thread execution**: Certain APIs must be called from the main thread
2. **CFRunLoop**: Some frameworks require a running `CFRunLoop` on the main thread
3. **Code signing**: Binaries must be signed with specific entitlements (e.g., `com.apple.security.virtualization`)

These requirements conflict with `#[tokio::main]`, which takes ownership of the main thread for async execution.

### Current State of the Art

Every existing Rust crate using Apple hypervisor/virtualization APIs punts on this problem:

```bash
# Build
cargo build --release

# Manually sign
codesign --sign - --entitlements entitlements.xml --deep --force target/release/myapp

# Manually run
./target/release/myapp
```

This breaks `cargo run`, `cargo test`, and `cargo bench`, making development painful.

---

## Design Goals

1. **Ergonomic**: As close as possible to standard Rust/tokio code
2. **Cross-platform**: Compiles and works on non-Apple targets as a transparent passthrough
3. **Complete**: Handles `main`, tests, benchmarks, and code signing
4. **Minimal boilerplate**: No `test_harness::main!()` or similar ceremony
5. **Testable**: The crate itself must be testable, including Apple-specific code paths

---

## Architecture Overview

```
apple-main/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API, re-exports
│   ├── runtime.rs          # Tokio + CFRunLoop coordination
│   ├── dispatch.rs         # GCD helpers for main thread dispatch
│   └── platform/
│       ├── mod.rs
│       ├── apple.rs        # macOS/iOS implementation
│       └── other.rs        # Passthrough for non-Apple
├── macros/                 # Proc macro crate (apple-main-macros)
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs          # #[apple_main::main], #[apple_main::test]
├── codesign-run/           # Binary for cargo runner
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
└── entitlements/           # Common entitlement files
    ├── virtualization.entitlements
    ├── hypervisor.entitlements
    └── combined.entitlements
```

---

## Implementation Options (Ranked by Ergonomics)

### Option 1: Main Thread Dispatch (Preferred if Sufficient)

**Hypothesis**: Many Apple APIs don't need a *spinning* CFRunLoop — they just need to be *called* from the main thread.

If true, we can use standard `#[tokio::main]` and dispatch specific calls:

```rust
use apple_main::on_main;

#[tokio::main]  // Standard tokio!
async fn main() {
    // This closure runs on the main thread via GCD
    let vm = on_main(|| {
        VZVirtualMachineConfiguration::new()
    }).await;
    
    // Back on tokio worker threads
    vm.start().await;
}
```

**Implementation**:

```rust
// src/dispatch.rs

#[cfg(target_os = "macos")]
pub async fn on_main<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    dispatch::Queue::main().exec_async(move || {
        let result = f();
        let _ = tx.send(result);
    });
    
    rx.await.expect("main thread task cancelled")
}

#[cfg(not(target_os = "macos"))]
pub async fn on_main<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    // On non-Apple platforms, just run inline
    f()
}
```

**Sync variant**:

```rust
pub fn on_main_sync<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    #[cfg(target_os = "macos")]
    {
        dispatch::Queue::main().exec_sync(f)
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        f()
    }
}
```

**Pros**:
- Works with standard `#[tokio::main]` and `#[tokio::test]`
- No custom test harness needed
- Minimal API surface
- Cross-platform by design

**Cons**:
- May not work if Virtualization.framework truly requires a spinning runloop
- Requires testing to validate

**Research Findings**:

Based on research into how Apple frameworks interact with dispatch queues and runloops:

1. **VZVirtualMachine creates its own dispatch queue**: The Go `vz` library documentation states: "A new dispatch queue will create when called this function. Every operation on the virtual machine must be done on that queue. The callbacks and delegate methods are invoked on that queue." This suggests basic VM operations don't require the main queue.

2. **GUI features likely need CFRunLoop**: A QEMU patch notes: "Various macOS system libraries, including the Cocoa UI and anything using libdispatch, such as ParavirtualizedGraphics... only work when events are being handled on the main runloop."

3. **DispatchQueue.main requires draining**: From Swift forums: "@MainActor annotations and DispatchQueue.main only work if something is draining the main dispatch queue. Normally, calling RunLoop.main.run() or dispatchMain() on the main thread would be enough."

**Likely outcome**: 
- **Option 1 sufficient for**: VM creation, configuration, start/stop, serial console, socket communication, file sharing
- **Option 2 required for**: VZVirtualMachineView (GUI), ParavirtualizedGraphics, any UI-related virtualization features

---

## Handling Async Code with on_main()

Apple's Virtualization.framework uses **ObjC completion handlers**, not Rust `async/await`. This affects how we bridge the two worlds.

### Simple case: Sync API calls

```rust
#[tokio::main]
async fn main() {
    // VZVirtualMachine::new() is synchronous - returns immediately
    let vm = apple_main::on_main(|| {
        let config = VZVirtualMachineConfiguration::new();
        config.set_cpu_count(4);
        config.set_memory_size(4 * 1024 * 1024 * 1024);
        VZVirtualMachine::new(config)
    }).await;
}
```

### Completion handler APIs: Bridge back to tokio

```rust
#[tokio::main]
async fn main() {
    let vm = apple_main::on_main(|| create_vm_sync()).await;
    
    // vm.start() takes a completion handler - bridge it to async
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    apple_main::on_main(move || {
        vm.start_with_completion_handler(move |error| {
            // This fires on VM's internal dispatch queue (not main)
            let _ = tx.send(error);
        });
    }).await;
    
    // Back in async Rust
    match rx.await.unwrap() {
        Some(err) => panic!("VM failed to start: {:?}", err),
        None => println!("VM started successfully"),
    }
}
```

### If your existing code is Rust async

If you have existing async functions that wrap completion handlers (like capsa might), you have options:

**Option A: Restructure to separate sync/async concerns**

```rust
// Before: monolithic async function
async fn create_and_start_vm() -> Result<Vm> {
    let config = VmConfig::new().await?;  // Maybe this doesn't need main thread
    let vm = Vm::new(config).await?;      // This needs main thread
    vm.start().await?;                     // This needs main thread
    Ok(vm)
}

// After: explicit main-thread boundaries
async fn create_and_start_vm() -> Result<Vm> {
    let config = VmConfig::new().await?;  // Stays async, off main thread
    
    // Only the ObjC calls go to main thread
    let vm = apple_main::on_main(|| Vm::new_sync(config)).await;
    
    let (tx, rx) = tokio::sync::oneshot::channel();
    apple_main::on_main(move || {
        vm.start_with_handler(move |r| { let _ = tx.send(r); });
    }).await;
    rx.await??;
    
    Ok(vm)
}
```

**Option B: Add `on_main_async` helper** (more complex)

```rust
/// Run an async closure, ensuring main-thread work happens on main thread
/// but async machinery stays on tokio
pub async fn on_main_async<F, Fut, R>(f: F) -> R
where
    F: FnOnce(MainThreadHandle) -> Fut + Send + 'static,
    Fut: Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    // Implementation would need to coordinate between:
    // 1. GCD main queue for actual ObjC calls
    // 2. Tokio runtime for async coordination
    // Complex but doable
}

// Usage
let vm = apple_main::on_main_async(|main| async move {
    let config = prepare_config().await;  // tokio
    let vm = main.run(|| Vm::new(config));  // GCD main
    main.run(|| vm.start()).await;  // GCD main + completion handler
    vm
}).await;
```

**Option C: Use block_on inside on_main (simplest, but blocking)**

```rust
let vm = apple_main::on_main(|| {
    // Block the main thread while running async code
    // Works, but defeats some async benefits
    apple_main::runtime().block_on(async {
        create_and_start_vm().await
    })
}).await;
```

**Recommendation**: Start with Option A (explicit restructuring). It's the most transparent and easiest to reason about. The main-thread requirement typically only affects the actual ObjC API calls, not your Rust business logic.

---

## Validation Test

Before building the full crate, run this test to confirm Option 1 works for your use case:

```rust
// validation_test.rs
// 
// Build: rustc validation_test.rs -o validation_test
// Sign:  codesign --sign - --entitlements entitlements.xml --force validation_test
// Run:   ./validation_test

use std::sync::mpsc;
use std::time::Duration;

fn main() {
    println!("Testing Virtualization.framework with GCD dispatch (no CFRunLoop)...\n");
    
    let (tx, rx) = mpsc::channel();
    
    // Spawn a thread that will dispatch work to main
    std::thread::spawn(move || {
        // Import dispatch crate or use raw FFI
        // dispatch::Queue::main().exec_sync(|| { ... });
        
        // For this test, we'll use ObjC runtime directly
        unsafe {
            // Get main queue
            let main_queue: *mut std::ffi::c_void = dispatch_get_main_queue();
            
            // Dispatch sync to main
            dispatch_sync_f(main_queue, std::ptr::null_mut(), test_vz_on_main);
        }
        
        tx.send(()).unwrap();
    });
    
    // Give the dispatch a moment, then run main queue briefly
    std::thread::sleep(Duration::from_millis(100));
    
    // Process main queue events (without full CFRunLoop)
    unsafe {
        // One-shot drain of main queue
        let main_queue = dispatch_get_main_queue();
        dispatch_main();  // This actually blocks forever, so we use a different approach:
    }
    
    // Alternative: use CFRunLoopRunInMode with a short timeout
    // to drain pending blocks without blocking forever
    unsafe {
        CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, true);
    }
    
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(()) => println!("\n✅ Test passed! Option 1 is viable."),
        Err(_) => println!("\n❌ Test timed out. May need Option 2."),
    }
}

extern "C" fn test_vz_on_main(_ctx: *mut std::ffi::c_void) {
    println!("  Running on main thread: {}", is_main_thread());
    
    // Try to create a VZVirtualMachineConfiguration
    // This is the critical test - does it work without CFRunLoop spinning?
    
    unsafe {
        let cls = objc::runtime::Class::get("VZVirtualMachineConfiguration").unwrap();
        let config: *mut objc::runtime::Object = objc::msg_send![cls, new];
        
        if !config.is_null() {
            println!("  ✅ Created VZVirtualMachineConfiguration");
            
            // Try setting properties
            let _: () = objc::msg_send![config, setCPUCount: 2u64];
            let _: () = objc::msg_send![config, setMemorySize: 2u64 * 1024 * 1024 * 1024];
            
            println!("  ✅ Set CPU and memory configuration");
            
            // Try validation
            let mut error: *mut objc::runtime::Object = std::ptr::null_mut();
            let valid: bool = objc::msg_send![config, 
                validateWithError: &mut error as *mut *mut objc::runtime::Object];
            
            if valid {
                println!("  ✅ Configuration validated successfully");
            } else {
                println!("  ⚠️  Configuration validation failed (expected without boot loader)");
            }
            
            let _: () = objc::msg_send![config, release];
        } else {
            println!("  ❌ Failed to create VZVirtualMachineConfiguration");
        }
    }
}

fn is_main_thread() -> bool {
    unsafe { pthread_main_np() != 0 }
}

// FFI declarations
#[link(name = "dispatch")]
extern "C" {
    fn dispatch_get_main_queue() -> *mut std::ffi::c_void;
    fn dispatch_sync_f(
        queue: *mut std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: extern "C" fn(*mut std::ffi::c_void),
    );
    fn dispatch_main() -> !;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, returnAfterSourceHandled: bool) -> i32;
    static kCFRunLoopDefaultMode: CFStringRef;
}

type CFStringRef = *const std::ffi::c_void;

#[link(name = "pthread")]
extern "C" {
    fn pthread_main_np() -> std::ffi::c_int;
}
```

**Simpler version using the `dispatch` and `objc` crates:**

```rust
// Cargo.toml:
// [dependencies]
// dispatch = "0.2"
// objc = "0.2"
// 
// [target.'cfg(target_os = "macos")'.dependencies]
// core-foundation = "0.10"

use dispatch::Queue;
use std::sync::mpsc;

fn main() {
    let (tx, rx) = mpsc::channel();
    
    std::thread::spawn(move || {
        Queue::main().exec_sync(|| {
            println!("On main thread: {}", std::thread::current().name().unwrap_or("main"));
            
            // Test VZ APIs here
            test_virtualization_framework();
        });
        tx.send(()).unwrap();
    });
    
    // Need to drain main queue - this is the key question!
    // Option 1 hypothesis: exec_sync should work even without CFRunLoop spinning
    // because GCD handles it internally
    
    // Wait a moment then check if it completed
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    // If we're here and rx has a message, Option 1 works
    // If it hangs, we need CFRunLoop (Option 2)
    
    match rx.try_recv() {
        Ok(()) => println!("✅ Option 1 viable - dispatch completed without CFRunLoop"),
        Err(mpsc::TryRecvError::Empty) => {
            println!("⏳ Dispatch pending - trying CFRunLoop drain...");
            // Try running the loop briefly
            unsafe {
                core_foundation::runloop::CFRunLoopRunInMode(
                    core_foundation::runloop::kCFRunLoopDefaultMode,
                    0.1,
                    true
                );
            }
            match rx.try_recv() {
                Ok(()) => println!("✅ Works with brief CFRunLoop drain"),
                Err(_) => println!("❌ Need full CFRunLoop - use Option 2"),
            }
        }
        Err(mpsc::TryRecvError::Disconnected) => println!("❌ Channel disconnected"),
    }
}

fn test_virtualization_framework() {
    use objc::{class, msg_send, sel, sel_impl};
    
    unsafe {
        let cls = class!(VZVirtualMachineConfiguration);
        let config: *mut objc::runtime::Object = msg_send![cls, new];
        
        if !config.is_null() {
            println!("  ✅ VZVirtualMachineConfiguration created");
            let _: () = msg_send![config, release];
        } else {
            println!("  ❌ Failed to create config");
        }
    }
}
```

**What to look for:**
- If the dispatch completes without needing `CFRunLoopRunInMode`, Option 1 fully works
- If it needs the brief CFRunLoop drain, Option 1 works but needs a one-time setup
- If it hangs completely, you need Option 2 (full CFRunLoop spinning)

**⚠️ Action Item**: Empirically test with Virtualization.framework to confirm. Create a minimal test that:
1. Creates VZVirtualMachineConfiguration
2. Creates VZVirtualMachine  
3. Starts the VM
4. All via `dispatch::Queue::main().exec_sync()` without spinning CFRunLoop

If this works, Option 1 covers the majority of headless VM use cases.

---

### Option 2: Hybrid Runtime with Proc Macros

If Option 1 is insufficient and CFRunLoop must spin, we need to own `main()`:

```rust
use apple_main::prelude::*;

#[apple_main::main]
async fn main() {
    let vm = VZVirtualMachine::new();
    vm.start().await;
}
```

**Macro expansion (Apple)**:

```rust
fn main() {
    // Initialize tokio on background threads
    let rt = ::apple_main::init_runtime();
    
    // Spawn user's async main
    rt.spawn(async {
        let vm = VZVirtualMachine::new();
        vm.start().await;
    });
    
    // Main thread runs Apple's event loop
    ::apple_main::run_main_loop();
}
```

**Macro expansion (non-Apple)**:

```rust
fn main() {
    ::tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let vm = VZVirtualMachine::new();
            vm.start().await;
        })
}
```

**Runtime module**:

```rust
// src/runtime.rs

use std::sync::OnceLock;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn init_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime")
    })
}

pub fn runtime() -> &'static Runtime {
    RUNTIME.get().expect("runtime not initialized - use #[apple_main::main]")
}

#[cfg(target_os = "macos")]
pub fn run_main_loop() -> ! {
    unsafe {
        core_foundation::runloop::CFRunLoopRun();
    }
    unreachable!("CFRunLoopRun returned")
}

#[cfg(not(target_os = "macos"))]
pub fn run_main_loop() -> ! {
    // On non-Apple, block on a future that never completes
    // (user code should exit via std::process::exit or return)
    std::thread::park();
    unreachable!()
}
```

**Pros**:
- Full control over main thread
- Works with any Apple framework requirement

**Cons**:
- Custom macro instead of `#[tokio::main]`
- Need to handle graceful shutdown (can't just return from async main)

---

### Option 3: Test Harness Integration

For `#[test]` functions, Rust's test harness owns `main()`. We have two sub-options:

#### Option 3a: Works if Option 1 is sufficient

If `dispatch::Queue::main()` works, standard `#[tokio::test]` just works:

```rust
#[tokio::test]
async fn test_vm_creation() {
    let config = apple_main::on_main(|| {
        VZVirtualMachineConfiguration::new()
    }).await;
    
    assert!(config.is_valid());
}
```

No custom test harness needed!

#### Option 3b: Custom test harness (if CFRunLoop is required)

If tests need CFRunLoop, we must use `harness = false`:

```toml
# Cargo.toml
[[test]]
name = "integration"
harness = false
```

**Option 3b-i: Module-level attribute (cleanest custom harness)**:

```rust
// tests/integration.rs

#[apple_main::test_module]
mod vm_tests {
    use super::*;
    
    #[apple_main::test]
    async fn test_boot() {
        // ...
    }
    
    #[apple_main::test]
    async fn test_snapshot() {
        // ...
    }
}

// The attribute generates main() at the end of the file
```

The `#[apple_main::test_module]` macro:
1. Walks the module finding `#[apple_main::test]` functions
2. Registers them with `inventory`
3. Appends a `fn main()` that sets up runtime and uses `libtest-mimic`

**Option 3b-ii: Explicit registration (more transparent)**:

```rust
// tests/integration.rs

use apple_main::test_harness;

test_harness::register!(test_boot, test_snapshot);

#[apple_main::test]
async fn test_boot() { /* ... */ }

#[apple_main::test]
async fn test_snapshot() { /* ... */ }
```

**Implementation using libtest-mimic + inventory**:

```rust
// In apple-main-macros

#[proc_macro_attribute]
pub fn test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    let name = &func.sig.ident;
    let name_str = name.to_string();
    
    quote! {
        #func
        
        ::apple_main::inventory::submit!(::apple_main::TestCase {
            name: #name_str,
            func: || ::apple_main::block_on(#name()),
        });
    }.into()
}
```

```rust
// src/lib.rs

pub use inventory;

pub struct TestCase {
    pub name: &'static str,
    pub func: fn() -> Result<(), Box<dyn std::error::Error + Send + Sync>>,
}

inventory::collect!(TestCase);

pub fn run_tests() -> ! {
    let args = libtest_mimic::Arguments::from_args();
    
    let tests: Vec<_> = inventory::iter::<TestCase>
        .into_iter()
        .map(|tc| {
            libtest_mimic::Trial::test(tc.name, tc.func)
        })
        .collect();
    
    #[cfg(target_os = "macos")]
    {
        // Ensure main thread is available for Apple APIs
        init_runtime();
        libtest_mimic::run(&args, tests).exit();
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        libtest_mimic::run(&args, tests).exit();
    }
}

/// For use in test files with harness = false
#[macro_export]
macro_rules! test_main {
    () => {
        fn main() {
            ::apple_main::run_tests()
        }
    };
}
```

---

## Code Signing Solution

### The `codesign-run` Binary

A small binary that signs and executes in one step:

```rust
// codesign-run/src/main.rs

use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    // Parse our args vs forwarded args
    let mut entitlements = env::var("APPLE_MAIN_ENTITLEMENTS")
        .unwrap_or_else(|_| "entitlements.xml".to_string());
    let mut binary_idx = 1;
    
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--entitlements" | "-e" => {
                entitlements = args.get(i + 1)
                    .expect("--entitlements requires a path")
                    .clone();
                i += 2;
                binary_idx = i;
            }
            _ => break,
        }
    }
    
    let binary = &args[binary_idx];
    let binary_args = &args[binary_idx + 1..];
    
    // Sign the binary
    let status = Command::new("codesign")
        .args([
            "--sign", "-",
            "--entitlements", &entitlements,
            "--deep",
            "--force",
            binary,
        ])
        .status()
        .expect("failed to execute codesign");
    
    if !status.success() {
        eprintln!("codesign failed with status: {}", status);
        std::process::exit(1);
    }
    
    // Execute the binary (replaces current process)
    let err = Command::new(binary)
        .args(binary_args)
        .exec();
    
    // exec() only returns on error
    eprintln!("failed to execute {}: {}", binary, err);
    std::process::exit(1);
}
```

---

## Decision Tree: Which Option Do You Need?

```
┌─────────────────────────────────────────────────────────────┐
│         Does your project use VZVirtualMachineView          │
│            or ParavirtualizedGraphics (GUI)?                │
└─────────────────────────────────────────────────────────────┘
                           │
              ┌────────────┴────────────┐
              │                         │
             YES                        NO
              │                         │
              ▼                         ▼
┌─────────────────────────┐  ┌─────────────────────────────────┐
│  Use Option 2:          │  │  Use Option 1:                  │
│  #[apple_main::main]    │  │  #[tokio::main] + on_main()     │
│                         │  │                                 │
│  CFRunLoop must spin    │  │  Standard tokio, dispatch calls │
│  for GUI callbacks      │  │  to main thread as needed       │
└─────────────────────────┘  └─────────────────────────────────┘
```

**Option 1 is sufficient for:**
- Headless VM servers (e.g., CI runners, cloud VMs)
- CLI tools that manage VMs
- gRPC/HTTP services controlling VMs
- Serial console interaction
- Virtio socket communication
- File sharing (VZSharedDirectory)

**Option 2 is required for:**
- Desktop VM applications with GUI
- Apps using VZVirtualMachineView
- Apps using ParavirtualizedGraphics
- Any UI that displays VM state in real-time

### Cargo Configuration

Users add to their project:

```toml
# .cargo/config.toml

[target.aarch64-apple-darwin]
runner = "codesign-run"

[target.x86_64-apple-darwin]
runner = "codesign-run"

# With custom entitlements:
# runner = ["codesign-run", "--entitlements", "my-entitlements.xml"]
```

Or via environment variable:

```bash
export CARGO_TARGET_AARCH64_APPLE_DARWIN_RUNNER="codesign-run"
export APPLE_MAIN_ENTITLEMENTS="path/to/entitlements.xml"
```

### Installation

```bash
cargo install apple-main --bin codesign-run
```

Or the crate could provide a setup command:

```bash
cargo apple-main init
# Creates .cargo/config.toml and entitlements.xml
```

---

## Benchmarking Support

### Criterion (Recommended)

Criterion works out of the box with the runner approach:

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

### Built-in `#[bench]` (Nightly)

Still unstable, same harness control issues as `#[test]`. Not recommended.

### Divan (Modern Alternative)

Also works with the runner approach:

```rust
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

---

## Cross-Platform Design

All Apple-specific code is behind `#[cfg(target_os = "macos")]`:

```rust
// src/lib.rs

mod platform;

#[cfg(target_os = "macos")]
pub use platform::apple::*;

#[cfg(not(target_os = "macos"))]
pub use platform::other::*;

// Common re-exports that work everywhere
pub use crate::dispatch::{on_main, on_main_sync};
```

```rust
// src/platform/other.rs

/// No-op runtime initialization for non-Apple platforms
pub fn init_runtime() -> &'static tokio::runtime::Runtime {
    // Just return a standard tokio runtime
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Runtime::new().unwrap()
    })
}

/// No-op on non-Apple
pub fn run_main_loop() -> ! {
    panic!("run_main_loop called on non-Apple platform")
}
```

The proc macros also check the target:

```rust
#[proc_macro_attribute]
pub fn main(attr: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as ItemFn);
    
    // Generate platform-specific expansion
    quote! {
        fn main() {
            #[cfg(target_os = "macos")]
            {
                ::apple_main::__internal::apple_main_impl(|| async {
                    #func
                });
            }
            
            #[cfg(not(target_os = "macos"))]
            {
                ::tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(async {
                        #func
                    });
            }
        }
    }.into()
}
```

---

## Testing apple-main Itself

### Unit Tests (No Apple APIs)

Standard tests for platform-independent logic:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_runtime_initialization() {
        let rt = super::init_runtime();
        assert!(rt.handle().is_some());
    }
}
```

### Integration Tests (Apple-Specific)

These need to run on macOS with signing:

```rust
// tests/apple_integration.rs

#![cfg(target_os = "macos")]

use apple_main::on_main;

#[tokio::test]
async fn test_dispatch_to_main() {
    let thread_id = on_main(|| {
        std::thread::current().id()
    }).await;
    
    // Verify it ran on a different thread (main thread)
    assert_ne!(thread_id, std::thread::current().id());
}
```

### CI Configuration

```yaml
# .github/workflows/ci.yml

jobs:
  test-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
  
  test-macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install codesign-run
        run: cargo install --path codesign-run
      - name: Configure runner
        run: |
          mkdir -p .cargo
          echo '[target.aarch64-apple-darwin]' >> .cargo/config.toml
          echo 'runner = "codesign-run"' >> .cargo/config.toml
      - run: cargo test
      - run: cargo test --features virtualization-tests
  
  test-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
```

### Feature Flags for Testing

```toml
# Cargo.toml

[features]
default = []

# Enable tests that require Virtualization.framework
virtualization-tests = []

# Enable tests that require Hypervisor.framework  
hypervisor-tests = []
```

```rust
#[cfg(all(
    target_os = "macos",
    feature = "virtualization-tests"
))]
#[tokio::test]
async fn test_vz_configuration() {
    let config = on_main(|| {
        // Actual Virtualization.framework code
    }).await;
}
```

---

## API Summary

### Core API

```rust
// For async contexts
pub async fn on_main<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static;

// For sync contexts
pub fn on_main_sync<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static;

// Runtime access
pub fn runtime() -> &'static tokio::runtime::Runtime;
pub fn init_runtime() -> &'static tokio::runtime::Runtime;

// Block on a future (uses runtime())
pub fn block_on<F: Future>(f: F) -> F::Output;
```

### Proc Macros (if Option 2 is needed)

```rust
#[apple_main::main]
async fn main() { }

#[apple_main::test]
async fn test_foo() { }
```

### Test Harness (if Option 3b is needed)

```rust
// In test file with harness = false
apple_main::test_main!();
```

---

## Migration Guide

### From `#[tokio::main]`

**If Option 1 works (preferred):**

```rust
// Before
#[tokio::main]
async fn main() {
    let vm = create_vm();  // ❌ Fails: not on main thread
}

// After
#[tokio::main]
async fn main() {
    let vm = apple_main::on_main(|| create_vm()).await;  // ✅
}
```

**If Option 2 is needed:**

```rust
// Before
#[tokio::main]
async fn main() {
    // ...
}

// After
#[apple_main::main]
async fn main() {
    // ... (no changes to body needed)
}
```

### From manual codesign workflow

```bash
# Before
cargo build --release
codesign --sign - --entitlements ent.xml --force target/release/app
./target/release/app

# After
cargo run --release  # Just works!
```

---

## Open Questions

1. **Does Virtualization.framework work with `dispatch::Queue::main().exec_sync()`?**
   - **Likely answer based on research**: YES for headless operations (VM lifecycle, serial console, sockets, file sharing)
   - **Likely NO for**: GUI features (VZVirtualMachineView, ParavirtualizedGraphics)
   - **Action**: Empirical testing required to confirm

2. **Should we vendor `codesign-run` or require separate install?**
   - Separate install is cleaner for crate size
   - Could provide `cargo apple-main install-runner` command

3. **How to handle entitlements discovery?**
   - Search order: `APPLE_MAIN_ENTITLEMENTS` env → `./entitlements.xml` → bundled defaults
   - Could support `Cargo.toml` metadata

4. **Should we support iOS/tvOS/watchOS?**
   - Same patterns apply but different deployment story
   - Start with macOS, extend later

5. **Integration with objc2 crate ecosystem?**
   - They have `MainThreadMarker` concept
   - Could provide interop

6. **Should Option 1 and Option 2 coexist?**
   - Projects using GUI features need Option 2
   - Projects doing headless VMs can use simpler Option 1
   - Could detect at runtime and warn if mismatched

---

## Implementation Roadmap

### Phase 1: Core Runtime (MVP)
- [ ] `on_main()` / `on_main_sync()` dispatch helpers
- [ ] Basic runtime coordination
- [ ] Cross-platform passthrough
- [ ] Unit tests

### Phase 2: Code Signing
- [ ] `codesign-run` binary
- [ ] Bundled entitlements files
- [ ] Documentation for `.cargo/config.toml` setup

### Phase 3: Proc Macros (if needed)
- [ ] `#[apple_main::main]` macro
- [ ] `#[apple_main::test]` macro
- [ ] Test harness integration

### Phase 4: Polish
- [ ] `cargo apple-main init` setup command
- [ ] Comprehensive examples
- [ ] Integration with popular frameworks (tonic, axum, etc.)

---

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "sync"] }

[target.'cfg(target_os = "macos")'.dependencies]
dispatch = "0.2"
core-foundation = "0.10"
# Or use objc2-foundation for more modern bindings

[dev-dependencies]
libtest-mimic = "0.7"
inventory = "0.3"
```

---

## License

MIT OR Apache-2.0 (standard Rust dual-license)

---

## Distribution / Release Builds

The `codesign-run` runner handles signing for development workflows (`cargo run`, `cargo test`, `cargo bench`). **For distributing signed release binaries, additional steps are required.**

### Option A: Cargo Subcommand (Recommended)

A `cargo-apple-main` binary that wraps build + sign:

```bash
cargo install apple-main
cargo apple-main build --release
# Output at target/release/myapp is signed and ready to distribute
```

**Implementation** (`cargo-apple-main/src/main.rs`):

```rust
use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().skip(2).collect(); // skip "cargo" "apple-main"
    
    // Forward to cargo
    let status = Command::new("cargo")
        .args(&args)
        .status()
        .expect("cargo failed");
    
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    
    // If this was a build command, sign the outputs
    if args.iter().any(|a| a == "build" || a == "b") {
        sign_outputs(&args);
    }
}

fn sign_outputs(args: &[String]) {
    let release = args.iter().any(|a| a == "--release" || a == "-r");
    let profile = if release { "release" } else { "debug" };
    
    // Find and sign binaries in target/{profile}/
    // ... implementation details
}
```

### Option B: Just / Makefile / xtask

For projects preferring explicit control:

```just
# justfile
release:
    cargo build --release
    codesign --sign - --entitlements entitlements.xml --force target/release/{{name}}

release-dist identity:
    cargo build --release
    codesign --sign "{{identity}}" --entitlements entitlements.xml \
        --options runtime target/release/{{name}}
    # For notarization, add: xcrun notarytool submit ...
```

```makefile
# Makefile
release:
	cargo build --release
	codesign --sign - --entitlements entitlements.xml --force target/release/$(NAME)
```

### Option C: CI/CD Script

For GitHub Actions or similar:

```yaml
# .github/workflows/release.yml
jobs:
  build-macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Build
        run: cargo build --release
      
      - name: Sign for distribution
        env:
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
        run: |
          # Import certificate to keychain
          echo "$APPLE_CERTIFICATE" | base64 --decode > certificate.p12
          security create-keychain -p "" build.keychain
          security import certificate.p12 -k build.keychain -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
          security set-key-partition-list -S apple-tool:,apple: -s -k "" build.keychain
          
          # Sign with Developer ID
          codesign --sign "Developer ID Application: Your Name" \
            --entitlements entitlements.xml \
            --options runtime \
            target/release/myapp
      
      - name: Notarize (optional)
        run: |
          xcrun notarytool submit target/release/myapp \
            --apple-id "$APPLE_ID" \
            --password "$APP_SPECIFIC_PASSWORD" \
            --team-id "$TEAM_ID" \
            --wait
```

### Signing Requirements Summary

| Use Case | Signing | Entitlements | Notarization |
|----------|---------|--------------|--------------|
| Local development | Ad-hoc (`-`) | Required | No |
| Internal distribution | Ad-hoc or Developer ID | Required | Optional |
| Public distribution | Developer ID | Required | Required |
| App Store | Distribution cert | Required | Via App Store |

### Entitlements for Common Use Cases

**Virtualization.framework** (VMs):
```xml
<key>com.apple.security.virtualization</key>
<true/>
```

**Hypervisor.framework** (low-level):
```xml
<key>com.apple.security.hypervisor</key>
<true/>
```

**Bridged networking**:
```xml
<key>com.apple.vm.networking</key>
<true/>
```

**Combined** (most VM projects):
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.virtualization</key>
    <true/>
    <key>com.apple.security.hypervisor</key>
    <true/>
    <key>com.apple.vm.networking</key>
    <true/>
</dict>
</plist>
```
