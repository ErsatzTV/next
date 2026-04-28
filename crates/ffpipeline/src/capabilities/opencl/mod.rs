#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
pub(crate) mod cl;

#[cfg(not(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
)))]
pub(crate) mod stub;

#[derive(Debug, Clone, Default)]
pub struct OpenCLCapabilities {
    pub(crate) platform_count: u32,
    pub(crate) gpu_device_count: u32,
}

impl OpenCLCapabilities {
    pub fn can_tonemap(&self) -> bool {
        self.platform_count > 0 && self.gpu_device_count > 0
    }

    pub fn can_pad(&self) -> bool {
        self.platform_count > 0 && self.gpu_device_count > 0
    }
}
