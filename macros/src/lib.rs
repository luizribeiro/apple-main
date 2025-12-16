use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

/// Attribute macro for async main functions that need Apple framework support.
///
/// On macOS, this initializes the tokio runtime and runs the user's async main
/// on a background thread while keeping the main thread available for Apple APIs.
///
/// On non-macOS platforms, this is equivalent to `#[tokio::main]`.
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

/// Attribute macro for async test functions that need Apple framework support.
///
/// On non-macOS platforms, this is equivalent to `#[tokio::test]`.
/// On macOS, tests should use `on_main_sync` for main-thread operations.
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
