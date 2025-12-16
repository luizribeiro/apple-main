//! apple-main: Integrate async Rust with Apple's main-thread-bound frameworks
//!
//! This crate provides seamless integration between async Rust (tokio) and Apple's
//! main-thread-bound frameworks like Virtualization.framework and AppKit.
//!
//! # Cross-Platform Support
//!
//! All APIs work transparently on non-Apple platforms:
//! - `on_main()` / `on_main_sync()` execute inline (no thread switching)
//! - `is_main_thread()` always returns `true`
//! - `#[apple_main::main]` expands to standard `#[tokio::main]`
//!
//! This means you can write cross-platform code that "just works" everywhere.

mod dispatch;
mod platform;
mod runtime;
mod test_harness;

pub use apple_main_macros::{harness_test, main, test};
pub use dispatch::{on_main, on_main_sync};
pub use runtime::{block_on, init_runtime, runtime};
pub use test_harness::{run_tests, TestCase};

#[cfg(target_os = "macos")]
pub use platform::apple::is_main_thread;

#[cfg(not(target_os = "macos"))]
pub use platform::other::is_main_thread;

pub use inventory;
pub use libtest_mimic;

#[cfg(feature = "criterion")]
pub use criterion;

#[doc(hidden)]
pub mod __internal {
    #[cfg(target_os = "macos")]
    pub fn run_main_loop() -> ! {
        // SAFETY: CFRunLoopRun is safe to call from the main thread.
        // This function is designed to be the main thread's blocking event loop.
        // It has no preconditions beyond being called from a thread with a runloop.
        unsafe {
            CFRunLoopRun();
        }
        unreachable!("CFRunLoopRun returned")
    }

    #[cfg(target_os = "macos")]
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRunLoopRun();
    }

    #[cfg(not(target_os = "macos"))]
    pub fn run_main_loop() -> ! {
        panic!("run_main_loop should not be called on non-macOS platforms")
    }
}

/// Macro to generate a main function for test files with `harness = false`.
///
/// # Example
///
/// ```ignore
/// // tests/my_tests.rs (with harness = false in Cargo.toml)
///
/// #[apple_main::harness_test]
/// async fn test_vm_creation() {
///     // Your test code - can use on_main() since CFRunLoop is active
///     let config = apple_main::on_main(|| {
///         VZVirtualMachineConfiguration::new()
///     }).await;
/// }
///
/// apple_main::test_main!();
/// ```
#[macro_export]
macro_rules! test_main {
    () => {
        fn main() {
            $crate::run_tests();
        }
    };
}

/// Macro to generate a main function for Criterion benchmarks.
///
/// This replaces `criterion_main!` and handles CFRunLoop setup on macOS
/// so that `on_main_sync()` works correctly in benchmarks.
///
/// # Example
///
/// ```ignore
/// use apple_main::criterion::{criterion_group, Criterion};
///
/// fn vm_benchmark(c: &mut Criterion) {
///     c.bench_function("vm_create", |b| {
///         b.iter(|| {
///             apple_main::on_main_sync(|| {
///                 VZVirtualMachineConfiguration::new()
///             })
///         })
///     });
/// }
///
/// criterion_group!(benches, vm_benchmark);
/// apple_main::criterion_main!(benches);
/// ```
#[cfg(feature = "criterion")]
#[macro_export]
macro_rules! criterion_main {
    ($($group:path),+ $(,)?) => {
        fn main() {
            #[cfg(target_os = "macos")]
            {
                // Spawn Criterion on background thread
                ::std::thread::spawn(|| {
                    // Wait for main runloop to start
                    ::std::thread::sleep(::std::time::Duration::from_millis(100));

                    // Run benchmark groups
                    $($group();)+

                    // Final summary
                    $crate::criterion::Criterion::default().final_summary();

                    // Stop main runloop when done
                    ::dispatch::Queue::main().exec_async(|| {
                        ::core_foundation::runloop::CFRunLoop::get_current().stop();
                    });
                });

                // Main thread runs CFRunLoop (drains dispatch queue)
                ::core_foundation::runloop::CFRunLoop::run_current();
            }

            #[cfg(not(target_os = "macos"))]
            {
                // On other platforms, just run Criterion normally
                $($group();)+
                $crate::criterion::Criterion::default().final_summary();
            }
        }
    };
}
