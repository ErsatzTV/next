#![allow(non_upper_case_globals)]

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub mod probe;
