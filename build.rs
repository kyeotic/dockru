use std::process::Command;

fn main() {
    // Try env var first (set from Docker build arg or CI environment)
    let sha = std::env::var("GIT_COMMIT_SHA")
        .ok()
        .filter(|s| !s.is_empty() && s != "unknown")
        .unwrap_or_else(|| {
            // Fall back to running git
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        });

    println!("cargo:rustc-env=GIT_COMMIT_SHA={}", sha);
    println!("cargo:rerun-if-changed=.git/HEAD");
}
