use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn init_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime")
    })
}

pub fn runtime() -> &'static Runtime {
    RUNTIME.get().expect(
        "runtime not initialized - call init_runtime() before using runtime() or block_on()",
    )
}

pub fn block_on<F: Future>(f: F) -> F::Output {
    runtime().block_on(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_runtime_creates_runtime() {
        let rt = init_runtime();
        let _ = rt.handle();
    }

    #[test]
    fn init_runtime_returns_same_instance() {
        let rt1 = init_runtime();
        let rt2 = init_runtime();
        assert!(std::ptr::eq(rt1, rt2));
    }

    #[test]
    fn block_on_executes_future() {
        init_runtime();
        let result = block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn block_on_with_spawn() {
        init_runtime();
        let result = block_on(async {
            let handle = tokio::spawn(async { 100 });
            handle.await.unwrap()
        });
        assert_eq!(result, 100);
    }

    #[test]
    fn concurrent_init_returns_same_runtime() {
        let handles: Vec<_> = (0..10)
            .map(|_| std::thread::spawn(|| init_runtime() as *const Runtime as usize))
            .collect();

        let addrs: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        assert!(addrs.windows(2).all(|w| w[0] == w[1]));
    }
}
