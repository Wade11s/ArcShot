fn main() {
    // Third-party SCK/apple-* bridges link Apple's OS-provided Swift runtime.
    // Rust's final link otherwise omits Swift's standard runtime search path.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
