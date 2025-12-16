use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[apple_main::harness_test]
async fn test_first() {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    assert_eq!(TEST_COUNTER.load(Ordering::SeqCst), 1);
}

#[apple_main::harness_test]
async fn test_second() {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
}

#[apple_main::harness_test]
async fn test_async_operation() {
    let result = async { 42 }.await;
    assert_eq!(result, 42);
}

#[apple_main::harness_test]
async fn test_is_main_thread_available() {
    let is_main = apple_main::is_main_thread();
    // Tests run on tokio threads, not main (CFRunLoop runs on main for dispatch)
    assert!(!is_main);
}

#[apple_main::harness_test]
async fn test_on_main_dispatch() {
    let result = apple_main::on_main(|| 42).await;
    assert_eq!(result, 42);
}

apple_main::test_main!();
