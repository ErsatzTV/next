fn main() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_arch == "x86" || target_arch == "x86_64" {
        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();

        if target_os == "windows" {
            #[cfg(target_os = "windows")]
            {
                if vcpkg::probe_package("vpl").is_ok() {
                    return;
                }
            }

            pkg_config::Config::new()
                .atleast_version("2.0")
                .probe("vpl")
                .expect("oneVPL (vpl) not found.");
        } else {
            pkg_config::Config::new()
                .atleast_version("2.0")
                .probe("vpl")
                .expect("oneVPL (libvpl) not found.");
        }
    }
}
