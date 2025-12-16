use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// Attribute macro for async main functions that need Apple framework support.
///
/// On macOS, this initializes the tokio runtime and runs the user's async main
/// on a background thread while keeping the main thread available for Apple APIs.
///
/// On non-macOS platforms, this is equivalent to `#[tokio::main]`.
///
/// # Example
///
/// ```ignore
/// #[apple_main::main]
/// async fn main() {
///     let config = apple_main::on_main(|| {
///         // This runs on the main thread
///         VZVirtualMachineConfiguration::new()
///     }).await;
/// }
/// ```
#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_block = &input.block;

    let expanded = quote! {
        fn main() {
            #[cfg(target_os = "macos")]
            {
                let rt = ::apple_main::init_runtime();
                rt.spawn(async #fn_block);
                ::apple_main::__internal::run_main_loop();
            }

            #[cfg(not(target_os = "macos"))]
            {
                ::tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("failed to create tokio runtime")
                    .block_on(async #fn_block);
            }
        }
    };

    expanded.into()
}

/// Attribute macro for async test functions.
///
/// This works with the standard cargo test harness. For tests that need
/// a custom harness with main thread support, use `#[apple_main::harness_test]`
/// with `harness = false` in Cargo.toml.
///
/// # Example
///
/// ```ignore
/// #[apple_main::test]
/// async fn test_something() {
///     let result = some_async_operation().await;
///     assert!(result.is_ok());
/// }
/// ```
#[proc_macro_attribute]
pub fn test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_block = &input.block;

    let expanded = quote! {
        #[test]
        fn #fn_name() {
            ::apple_main::init_runtime();
            ::apple_main::block_on(async #fn_block);
        }
    };

    expanded.into()
}

/// Attribute macro for async test functions that register with the custom harness.
///
/// Use this with `harness = false` in Cargo.toml and `test_main!()` macro.
/// This allows tests to run with proper main thread support on macOS.
///
/// # Example
///
/// ```ignore
/// // Cargo.toml:
/// // [[test]]
/// // name = "my_tests"
/// // harness = false
///
/// // tests/my_tests.rs:
/// #[apple_main::harness_test]
/// async fn test_vm_creation() {
///     let config = apple_main::on_main(|| {
///         VZVirtualMachineConfiguration::new()
///     }).await;
///     assert!(config.is_valid());
/// }
///
/// apple_main::test_main!();
/// ```
#[proc_macro_attribute]
pub fn harness_test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let fn_name = &input.sig.ident;
    let fn_name_str = fn_name.to_string();
    let fn_block = &input.block;

    let expanded = quote! {
        fn #fn_name() -> ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = ()> + Send>> {
            ::std::boxed::Box::pin(async #fn_block)
        }

        ::apple_main::inventory::submit!(::apple_main::TestCase {
            name: #fn_name_str,
            func: #fn_name,
        });
    };

    expanded.into()
}
