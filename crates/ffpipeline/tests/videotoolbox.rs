#![cfg(target_os = "macos")]
mod common;

use common::*;
use ffpipeline::accel::video_toolbox::VideoToolbox;
use ffpipeline::capabilities::videotoolbox::VideoToolboxCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::hw_accel::HardwareAccel;

fn probe() -> Option<HardwareAccel> {
    let capabilities = VideoToolboxCapabilities::probe().ok()?;
    (capabilities.count() > 0).then(|| HardwareAccel::VideoToolbox(VideoToolbox { capabilities }))
}

async fn accel() -> Option<HardwareAccel> {
    hardware_accel(KnownHardwareAccel::VideoToolbox, probe).await
}

shared_tests!(accel);
