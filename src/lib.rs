//! apple-main: Integrate async Rust with Apple's main-thread-bound frameworks

mod dispatch;
mod platform;
mod runtime;

pub use apple_main_macros::{main, test};
pub use dispatch::{on_main, on_main_sync};
pub use runtime::{block_on, init_runtime, runtime};

#[cfg(target_os = "macos")]
pub use platform::apple::is_main_thread;

#[cfg(not(target_os = "macos"))]
pub use platform::other::is_main_thread;

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
