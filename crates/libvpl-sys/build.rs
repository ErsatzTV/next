fn main() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "x86" || target_arch == "x86_64" {
        pkg_config::Config::new()
            .atleast_version("2.0")
            .probe("vpl")
            .expect("oneVPL (libvpl) not found. Please install the oneVPL-devel package.");
    }
}
