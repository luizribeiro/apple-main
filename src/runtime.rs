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
