#![cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
mod common;

use common::*;
use ffpipeline::accel::qsv::Qsv;
use ffpipeline::capabilities::qsv::QsvCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::hw_accel::HardwareAccel;

fn probe() -> Option<HardwareAccel> {
    let capabilities = QsvCapabilities::probe().ok()?;
    (capabilities.count() > 0).then(|| HardwareAccel::Qsv(Qsv { capabilities }))
}

async fn accel() -> Option<HardwareAccel> {
    hardware_accel(KnownHardwareAccel::Qsv, probe).await
}

shared_tests!(accel);
