use std::future::Future;
use std::pin::Pin;

/// A test case registered with the custom test harness.
pub struct TestCase {
    pub name: &'static str,
    pub func: fn() -> Pin<Box<dyn Future<Output = ()> + Send>>,
}

inventory::collect!(TestCase);

fn collect_tests() -> Vec<libtest_mimic::Trial> {
    inventory::iter::<TestCase>
        .into_iter()
        .map(|tc| {
            let func = tc.func;
            libtest_mimic::Trial::test(tc.name, move || {
                crate::block_on(func());
                Ok(())
            })
        })
        .collect()
}

/// Run all registered tests using libtest-mimic.
///
/// On macOS, this starts CFRunLoop on the main thread so that `on_main()` and
/// `on_main_sync()` work correctly. Tests run on the tokio runtime.
///
/// This function is called by the `test_main!()` macro.
#[cfg(target_os = "macos")]
pub fn run_tests() -> ! {
    let args = libtest_mimic::Arguments::from_args();
    let tests = collect_tests();

    let rt = crate::init_runtime();

    rt.spawn(async move {
        libtest_mimic::run(&args, tests).exit();
    });

    crate::__internal::run_main_loop();
}

/// Run all registered tests using libtest-mimic.
///
/// On non-macOS platforms, this simply runs tests on the tokio runtime.
///
/// This function is called by the `test_main!()` macro.
#[cfg(not(target_os = "macos"))]
pub fn run_tests() -> ! {
    let args = libtest_mimic::Arguments::from_args();
    let tests = collect_tests();

    crate::init_runtime();
    libtest_mimic::run(&args, tests).exit();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_can_be_created() {
        let _tc = TestCase {
            name: "test",
            func: || Box::pin(async {}),
        };
    }
}
