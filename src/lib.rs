//! apple-main: Integrate async Rust with Apple's main-thread-bound frameworks

mod dispatch;
mod platform;
mod runtime;

pub use dispatch::{on_main, on_main_sync};
pub use runtime::{block_on, init_runtime, runtime};

#[cfg(target_os = "macos")]
pub use platform::apple::is_main_thread;

#[cfg(not(target_os = "macos"))]
pub use platform::other::is_main_thread;
