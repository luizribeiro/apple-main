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
