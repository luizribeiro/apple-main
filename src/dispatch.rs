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

    rx.await.expect(
        "main thread dispatch failed: the main thread dropped the task before completion. \
         This likely indicates the main dispatch queue is not running or the process is shutting down.",
    )
}

#[cfg(not(target_os = "macos"))]
pub async fn on_main<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    f()
}

#[cfg(target_os = "macos")]
pub fn on_main_sync<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    dispatch::Queue::main().exec_sync(f)
}

#[cfg(not(target_os = "macos"))]
pub fn on_main_sync<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    f()
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "macos"))]
    mod non_macos {
        use crate::{on_main, on_main_sync};

        #[tokio::test]
        async fn on_main_returns_value() {
            let result = on_main(|| 42).await;
            assert_eq!(result, 42);
        }

        #[tokio::test]
        async fn on_main_executes_closure() {
            let result = on_main(|| String::from("hello")).await;
            assert_eq!(result, "hello");
        }

        #[test]
        fn on_main_sync_returns_value() {
            let result = on_main_sync(|| 42);
            assert_eq!(result, 42);
        }

        #[test]
        fn on_main_sync_executes_closure() {
            let result = on_main_sync(|| vec![1, 2, 3]);
            assert_eq!(result, vec![1, 2, 3]);
        }
    }

    #[cfg(target_os = "macos")]
    mod macos {
        // NOTE: on_main_sync tests are commented out because they require an active
        // main dispatch queue, which test harnesses don't provide. The dispatch to
        // the main queue will block forever waiting for a runloop that isn't running.
        //
        // These functions work correctly in actual applications where the main thread
        // has an active runloop (e.g., GUI apps or apps using CFRunLoop/NSRunLoop).
        //
        // To test: use integration tests with a proper main loop setup.

        #[test]
        fn module_compiles() {
            // Verify the module compiles with dispatch crate
            let _ = dispatch::Queue::main();
        }
    }
}
