mod common;

use common::*;
use ffpipeline::accel::rkmpp::Rkmpp;
use ffpipeline::capabilities::rkmpp::RkmppCapabilities;
use ffpipeline::ffmpeg_info::KnownHardwareAccel;
use ffpipeline::hw_accel::HardwareAccel;

fn probe() -> Option<HardwareAccel> {
    let capabilities = RkmppCapabilities::probe().ok()?;
    (capabilities.count() > 0).then(|| HardwareAccel::Rkmpp(Rkmpp { capabilities }))
}

async fn accel() -> Option<HardwareAccel> {
    hardware_accel(KnownHardwareAccel::Rkmpp, probe).await
}

shared_tests!(accel);
