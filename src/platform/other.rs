pub fn is_main_thread() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_main_thread_always_returns_true() {
        assert!(is_main_thread());
    }

    #[test]
    fn is_main_thread_returns_true_on_spawned_thread() {
        let handle = std::thread::spawn(is_main_thread);
        let result = handle.join().unwrap();
        assert!(result);
    }
}
