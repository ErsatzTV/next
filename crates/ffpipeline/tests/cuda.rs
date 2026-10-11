#![cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod common;

use common::*;
use ffpipeline::accel::cuda::Cuda;
use ffpipeline::capabilities::nvidia::NvidiaCapabilities;
use ffpipeline::capabilities::vulkan::VulkanCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::hw_accel::HardwareAccel;

fn probe() -> Option<HardwareAccel> {
    let capabilities = NvidiaCapabilities::probe().ok()?;
    if capabilities.count() == 0 {
        return None;
    }
    let vulkan = VulkanCapabilities::probe_for_nvidia(capabilities.device_uuid()).ok();
    Some(HardwareAccel::Cuda(Cuda::new(capabilities, vulkan)))
}

async fn accel() -> Option<HardwareAccel> {
    hardware_accel(KnownHardwareAccel::Cuda, probe).await
}

shared_tests!(accel);
