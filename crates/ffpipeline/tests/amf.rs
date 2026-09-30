#![cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod common;

use common::*;
use ffpipeline::accel::amf::Amf;
use ffpipeline::capabilities::amf::AmfCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::hw_accel::HardwareAccel;

fn probe() -> Option<HardwareAccel> {
    let capabilities = AmfCapabilities::probe().ok()?;
    (capabilities.count() > 0).then(|| HardwareAccel::Amf(Amf { capabilities }))
}

async fn accel() -> Option<HardwareAccel> {
    hardware_accel(KnownHardwareAccel::Amf, probe).await
}

shared_tests!(accel);
