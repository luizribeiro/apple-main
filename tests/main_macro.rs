#[apple_main::main]
async fn main() {
    let result = async { 42 }.await;
    assert_eq!(result, 42);

    let handle = tokio::spawn(async { 100 });
    let spawned_result = handle.await.unwrap();
    assert_eq!(spawned_result, 100);

    #[cfg(target_os = "macos")]
    {
        apple_main::on_main(|| {
            assert!(apple_main::is_main_thread());
        })
        .await;
    }
}
