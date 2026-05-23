fn main() {
    println!("cargo:rerun-if-env-changed=ETV_VERSION");
    let version = std::env::var("ETV_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=ETV_VERSION_STRING={version}");
}
