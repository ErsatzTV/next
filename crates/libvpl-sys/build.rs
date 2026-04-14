fn main() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "x86" || target_arch == "x86_64" {
        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();

        if target_os == "windows" {
            let dir = std::env::var("VPL_DIR")
                .expect("VPL_DIR must be set to the libvpl install prefix on Windows");
            println!("cargo:rustc-link-search=native={dir}/lib");
            println!("cargo:rustc-link-lib=vpl");
        } else {
            pkg_config::Config::new()
                .atleast_version("2.0")
                .probe("vpl")
                .expect("oneVPL (libvpl) not found.");
        }
    }
}
