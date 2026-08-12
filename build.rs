use std::process::Command;

fn main() {
    let version = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        // Tags are written `v1.3.0`, but a version string is not: `git
        // --version` and `cargo --version` both report a bare number, and
        // clap_mangen prefixes its own `v` in the man page's VERSION section.
        .map(|s| s.strip_prefix('v').unwrap_or(&s).to_string())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    println!("cargo:rustc-env=GWT_VERSION={version}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}
