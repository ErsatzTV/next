fn main() {
    pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("vpl")
        .expect("oneVPL (libvpl) not found. Please install the oneVPL-devel package.");
}
