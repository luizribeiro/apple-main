//! Test whether on_main() works with #[tokio::main]
//!
//! Run with: cargo run --example tokio_main_test
//!
//! Expected behavior:
//! - If this hangs: on_main() requires #[apple_main::main]
//! - If this completes: on_main() works with standard #[tokio::main]

use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("Testing on_main() with #[tokio::main]...");
    println!("Platform: {}", std::env::consts::OS);
    println!("Main thread ID: {:?}", std::thread::current().id());

    // Test 1: Simple on_main call with timeout
    println!("\n[Test 1] Calling on_main() with 5 second timeout...");

    let result = tokio::time::timeout(Duration::from_secs(5), async {
        apple_main::on_main(|| {
            println!("  -> Inside on_main closure");
            println!("  -> Thread ID: {:?}", std::thread::current().id());
            println!("  -> is_main_thread: {}", apple_main::is_main_thread());
            42
        })
        .await
    })
    .await;

    match result {
        Ok(value) => {
            println!("  ✅ on_main() completed! Returned: {}", value);
        }
        Err(_) => {
            println!("  ❌ on_main() TIMED OUT - main queue not being drained");
            println!("\n  This means on_main() requires #[apple_main::main]");
            println!("  instead of #[tokio::main] on macOS.");
            return;
        }
    }

    // Test 2: Multiple sequential calls
    println!("\n[Test 2] Multiple sequential on_main() calls...");

    for i in 1..=3 {
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            apple_main::on_main(move || {
                println!("  -> Call {}", i);
                i
            })
            .await
        })
        .await;

        match result {
            Ok(v) => println!("  ✅ Call {} returned {}", i, v),
            Err(_) => {
                println!("  ❌ Call {} timed out", i);
                return;
            }
        }
    }

    // Test 3: Concurrent calls
    println!("\n[Test 3] Concurrent on_main() calls...");

    let results = tokio::time::timeout(Duration::from_secs(5), async {
        let handles: Vec<_> = (1..=3)
            .map(|i| tokio::spawn(async move { apple_main::on_main(move || i * 10).await }))
            .collect();

        let mut results = vec![];
        for h in handles {
            results.push(h.await.unwrap());
        }
        results
    })
    .await;

    match results {
        Ok(values) => println!("  ✅ Concurrent calls returned: {:?}", values),
        Err(_) => {
            println!("  ❌ Concurrent calls timed out");
            return;
        }
    }

    println!("\n========================================");
    println!("✅ All tests passed!");
    println!("on_main() works with #[tokio::main] on this platform.");
    println!("========================================");
}
