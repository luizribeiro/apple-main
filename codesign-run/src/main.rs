use std::env;
use std::os::unix::process::CommandExt;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut entitlements =
        env::var("APPLE_MAIN_ENTITLEMENTS").unwrap_or_else(|_| "entitlements.xml".to_string());
    let mut binary_idx = 1;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--entitlements" | "-e" => {
                entitlements = args
                    .get(i + 1)
                    .expect("--entitlements requires a path")
                    .clone();
                i += 2;
                binary_idx = i;
            }
            _ => break,
        }
    }

    if binary_idx >= args.len() {
        eprintln!("Usage: codesign-run [--entitlements <path>] <binary> [args...]");
        std::process::exit(1);
    }

    let binary = &args[binary_idx];
    let binary_args = &args[binary_idx + 1..];

    let status = Command::new("codesign")
        .args([
            "--sign",
            "-",
            "--entitlements",
            &entitlements,
            "--deep",
            "--force",
            binary,
        ])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Failed to execute codesign:");
            eprintln!("  Binary: {}", binary);
            eprintln!("  Entitlements: {}", entitlements);
            eprintln!("  Error: {}", e);
            std::process::exit(1);
        });

    if !status.success() {
        eprintln!("codesign failed:");
        eprintln!("  Binary: {}", binary);
        eprintln!("  Entitlements: {}", entitlements);
        eprintln!("  Exit status: {}", status);
        std::process::exit(1);
    }

    let err = Command::new(binary).args(binary_args).exec();

    eprintln!("failed to execute {}: {}", binary, err);
    std::process::exit(1);
}
