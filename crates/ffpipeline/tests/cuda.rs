#![cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod common;

use common::*;
use ffpipeline::accel::cuda::Cuda;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::hw_accel::HardwareAccel;

fn probe() -> Option<HardwareAccel> {
    let cuda = Cuda::probe().ok()?;
    if cuda.capabilities.count() == 0 {
        return None;
    }
    Some(HardwareAccel::Cuda(cuda))
}

async fn accel() -> Option<HardwareAccel> {
    hardware_accel(KnownHardwareAccel::Cuda, probe).await
}

shared_tests!(accel);
