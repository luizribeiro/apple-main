pub fn is_main_thread() -> bool {
    // SAFETY: pthread_main_np is a C function that's always safe to call.
    // It returns non-zero if the current thread is the main thread, zero otherwise.
    // No preconditions, no side effects, no memory safety concerns.
    unsafe { pthread_main_np() != 0 }
}

#[link(name = "pthread")]
extern "C" {
    fn pthread_main_np() -> std::ffi::c_int;
}
