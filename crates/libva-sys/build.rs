fn main() {
    pkg_config::probe_library("libva").unwrap();
    pkg_config::probe_library("libva-drm").unwrap();
}
