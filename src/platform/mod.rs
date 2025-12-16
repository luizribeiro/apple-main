#[cfg(target_os = "macos")]
pub mod apple;

#[cfg(not(target_os = "macos"))]
pub mod other;
