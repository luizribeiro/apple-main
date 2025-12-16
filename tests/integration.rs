use apple_main::{block_on, init_runtime};

#[test]
fn runtime_can_be_initialized() {
    let rt = init_runtime();
    let _ = rt.handle();
}

#[test]
fn block_on_runs_async_code() {
    init_runtime();
    let result = block_on(async { 42 });
    assert_eq!(result, 42);
}

#[cfg(not(target_os = "macos"))]
mod non_macos {
    use super::*;
    use apple_main::{on_main, on_main_sync};

    #[test]
    fn on_main_sync_executes_and_returns() {
        let result = on_main_sync(|| "hello from main");
        assert_eq!(result, "hello from main");
    }

    #[tokio::test]
    async fn on_main_async_returns_value() {
        let result = on_main(|| vec![1, 2, 3]).await;
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn on_main_async_with_complex_type() {
        let result = on_main(|| {
            let mut map = std::collections::HashMap::new();
            map.insert("key", 42);
            map
        })
        .await;
        assert_eq!(result.get("key"), Some(&42));
    }
}

#[cfg(target_os = "macos")]
mod macos {
    #[test]
    fn is_main_thread_works() {
        let is_main = apple_main::is_main_thread();
        // Test harness runs on worker threads, not main thread
        assert!(!is_main);
    }

    // NOTE: on_main_sync tests require a running main dispatch queue,
    // which the test harness doesn't provide. These are tested via
    // the unit tests in dispatch.rs that only verify compilation.
    // Full integration testing requires an application with a runloop.
}
