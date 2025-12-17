use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[apple_main::harness_test]
async fn test_counter_increments() {
    let before = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let after = TEST_COUNTER.load(Ordering::SeqCst);
    assert!(after > before, "counter should have incremented");
}

#[apple_main::harness_test]
async fn test_multiple_tests_run() {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    // Just verify this test runs - counter value depends on test order
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

#[apple_main::harness_test]
async fn test_on_main_sync_dispatch() {
    let result = apple_main::on_main_sync(|| 123);
    assert_eq!(result, 123);
}

apple_main::test_main!();
