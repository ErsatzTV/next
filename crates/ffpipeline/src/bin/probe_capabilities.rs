use clap::{Parser, Subcommand};
use ffpipeline::capabilities::nvidia::NvidiaCapabilities;
use ffpipeline::capabilities::opencl::OpenCLCapabilities;
use ffpipeline::capabilities::qsv::QsvCapabilities;
use ffpipeline::capabilities::vaapi::VaapiCapabilities;
use ffpipeline::capabilities::videotoolbox::VideoToolboxCapabilities;
use ffpipeline::capabilities::vulkan::VulkanCapabilities;
use ffpipeline::pipeline::{PixelFormat, VideoFormat};

#[derive(Parser)]
#[command(name = "probe-capabilities")]
#[command(about = "Probe and display hardware acceleration capabilities")]
struct Cli {
    #[command(subcommand)]
    accel: Accel,
}

#[derive(Subcommand)]
enum Accel {
    Cuda,
    Qsv,
    Vaapi {
        #[arg(long, default_value = "/dev/dri/renderD128")]
        device: String,
        #[arg(long)]
        driver: Option<String>,
    },
    VideoToolbox,
    Vulkan,
    Opencl,
}

const ALL_FORMATS: &[VideoFormat] = &[
    VideoFormat::Av1,
    VideoFormat::H264,
    VideoFormat::Hevc,
    VideoFormat::Mpeg2Video,
    VideoFormat::Vc1,
    VideoFormat::Vp8,
    VideoFormat::Vp9,
];

const VPP_FORMATS: &[PixelFormat] = &[
    PixelFormat::Nv12,
    PixelFormat::P010le,
    PixelFormat::Yuv420p,
    PixelFormat::Yuv420p10le,
    PixelFormat::Bgra,
];

fn yn(supported: bool) -> &'static str {
    if supported { "yes" } else { "-" }
}

fn format_name(f: &VideoFormat) -> &'static str {
    match f {
        VideoFormat::Av1 => "AV1",
        VideoFormat::H264 => "H.264",
        VideoFormat::Hevc => "HEVC",
        VideoFormat::Mpeg2Video => "MPEG-2",
        VideoFormat::Vc1 => "VC-1",
        VideoFormat::Vp8 => "VP8",
        VideoFormat::Vp9 => "VP9",
    }
}

fn pixel_format_name(f: &PixelFormat) -> &'static str {
    match f {
        PixelFormat::Nv12 => "nv12",
        PixelFormat::P010le => "p010le",
        PixelFormat::Yuv420p => "yuv420p",
        PixelFormat::Yuv420p10le => "yuv420p10le",
        PixelFormat::Bgra => "bgra",
        PixelFormat::Yuva420p => "yuva420p",
        PixelFormat::Yuva420p10le => "yuva420p10le",
        PixelFormat::P016 => "p016",
    }
}

fn main() {
    let cli = Cli::parse();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    let result = match cli.accel {
        Accel::Cuda => print_cuda(),
        Accel::Qsv => print_qsv(),
        Accel::Vaapi { device, driver } => print_vaapi(&device, driver.as_deref()),
        Accel::VideoToolbox => print_videotoolbox(),
        Accel::Vulkan => print_vulkan(),
        Accel::Opencl => print_opencl(),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn print_cuda() -> Result<(), String> {
    let caps = NvidiaCapabilities::probe().map_err(|e| e.to_string())?;
    println!("=== CUDA (NVIDIA) Capabilities ===");
    println!();

    print_decode_table(ALL_FORMATS, |f, bd| caps.can_decode(f, bd));

    println!();
    println!("Encoders:");
    println!(
        "  {:<12} {:<8} {:<8} {:<8}",
        "Codec", "8-bit", "10-bit", "B-Ref"
    );
    println!(
        "  {:<12} {:<8} {:<8} {:<8}",
        "-----", "-----", "------", "-----"
    );
    for f in ALL_FORMATS {
        let enc8 = caps.can_encode(f, 8);
        let enc10 = caps.can_encode(f, 10);
        if enc8 || enc10 {
            println!(
                "  {:<12} {:<8} {:<8} {:<8}",
                format_name(f),
                yn(enc8),
                yn(enc10),
                yn(caps.b_frame_ref_mode(f)),
            );
        }
    }

    println!();
    print_vpp_table(|pf| caps.vpp_supports_format(pf));

    Ok(())
}

fn print_qsv() -> Result<(), String> {
    let caps = QsvCapabilities::probe().map_err(|e| e.to_string())?;
    println!("=== QSV (Intel VPL) Capabilities ===");
    println!();

    print_decode_table(ALL_FORMATS, |f, bd| caps.can_decode(f, bd));
    println!();
    print_encode_table(ALL_FORMATS, |f, bd| caps.can_encode(f, bd));

    println!();
    print_vpp_table(|pf| caps.vpp_supports_format(pf));

    Ok(())
}

fn print_vaapi(device: &str, driver: Option<&str>) -> Result<(), String> {
    let caps = VaapiCapabilities::probe(device, driver).map_err(|e| e.to_string())?;
    println!("=== VAAPI Capabilities ===");
    println!("  Vendor:  {}", caps.vendor());
    println!("  Device:  {device}");
    println!("  Driver:  {}", driver.unwrap_or("(auto-detected)"));
    println!();

    print_decode_table(ALL_FORMATS, |f, bd| {
        let (codec, profile) = default_profile(f, bd);
        caps.can_decode(codec, profile, bd)
    });

    println!();
    println!("Encoders:");
    println!(
        "  {:<12} {:<8} {:<8} {:<8}",
        "Codec", "8-bit", "10-bit", "LP"
    );
    println!(
        "  {:<12} {:<8} {:<8} {:<8}",
        "-----", "-----", "------", "-----"
    );
    for f in ALL_FORMATS {
        let enc8 = caps.can_encode(f, 8);
        let enc10 = caps.can_encode(f, 10);
        let lp8 = caps.can_encode_low_power(f, 8);
        let lp10 = caps.can_encode_low_power(f, 10);
        if enc8 || enc10 || lp8 || lp10 {
            let lp_str = match (lp8, lp10) {
                (true, true) => "8+10",
                (true, false) => "8",
                (false, true) => "10",
                (false, false) => "-",
            };
            println!(
                "  {:<12} {:<8} {:<8} {:<8}",
                format_name(f),
                yn(enc8),
                yn(enc10),
                lp_str,
            );
        }
    }

    println!();
    print_vpp_table(|pf| caps.vpp_supports_format(pf));

    println!();
    println!("HDR Tone Mapping:");
    for pf in &[PixelFormat::Nv12, PixelFormat::P010le] {
        let hdr_sdr = caps.can_hdr_to_sdr_tonemap(pf);
        let hdr_hdr = caps.can_hdr_to_hdr_tonemap(pf);
        if hdr_sdr || hdr_hdr {
            println!(
                "  {:<14} HDR->SDR: {:<5} HDR->HDR: {:<5}",
                pixel_format_name(pf),
                yn(hdr_sdr),
                yn(hdr_hdr),
            );
        }
    }
    println!("  Overlay:     {}", yn(caps.can_overlay()));

    Ok(())
}

fn print_videotoolbox() -> Result<(), String> {
    let caps = VideoToolboxCapabilities::probe().map_err(|e| e.to_string())?;
    println!("=== VideoToolbox Capabilities ===");
    println!();

    print_decode_table(ALL_FORMATS, |f, bd| caps.can_decode(f, bd));
    println!();
    print_encode_table(ALL_FORMATS, |f, bd| caps.can_encode(f, bd));

    Ok(())
}

fn print_vulkan() -> Result<(), String> {
    let caps = VulkanCapabilities::probe().map_err(|e| e.to_string())?;
    println!("=== Vulkan Video Capabilities ===");
    println!();

    print_decode_table(ALL_FORMATS, |f, bd| caps.can_decode(f, bd));
    println!();
    print_encode_table(ALL_FORMATS, |f, bd| caps.can_encode(f, bd));

    Ok(())
}

fn print_opencl() -> Result<(), String> {
    let caps = OpenCLCapabilities::probe().map_err(|e| e.to_string())?;
    println!("=== OpenCL Video Capabilities ===");
    println!();

    println!("can_tonemap = {}", caps.can_tonemap());
    println!();
    println!("can_pad = {}", caps.can_pad());

    Ok(())
}

fn print_decode_table(formats: &[VideoFormat], can_decode: impl Fn(&VideoFormat, u8) -> bool) {
    println!("Decoders:");
    println!("  {:<12} {:<8} {:<8}", "Codec", "8-bit", "10-bit");
    println!("  {:<12} {:<8} {:<8}", "-----", "-----", "------");
    for f in formats {
        let dec8 = can_decode(f, 8);
        let dec10 = can_decode(f, 10);
        if dec8 || dec10 {
            println!("  {:<12} {:<8} {:<8}", format_name(f), yn(dec8), yn(dec10),);
        }
    }
}

fn print_encode_table(formats: &[VideoFormat], can_encode: impl Fn(&VideoFormat, u8) -> bool) {
    println!("Encoders:");
    println!("  {:<12} {:<8} {:<8}", "Codec", "8-bit", "10-bit");
    println!("  {:<12} {:<8} {:<8}", "-----", "-----", "------");
    for f in formats {
        let enc8 = can_encode(f, 8);
        let enc10 = can_encode(f, 10);
        if enc8 || enc10 {
            println!("  {:<12} {:<8} {:<8}", format_name(f), yn(enc8), yn(enc10),);
        }
    }
}

fn print_vpp_table(supports: impl Fn(&PixelFormat) -> bool) {
    println!("VPP Pixel Formats:");
    let mut any = false;
    for pf in VPP_FORMATS {
        if supports(pf) {
            println!("  {}", pixel_format_name(pf));
            any = true;
        }
    }
    if !any {
        println!("  (none detected)");
    }
}

fn default_profile(format: &VideoFormat, bit_depth: u8) -> (&'static str, &'static str) {
    match (format, bit_depth) {
        (VideoFormat::H264, _) => ("h264", "main"),
        (VideoFormat::Hevc, 10) => ("hevc", "main 10"),
        (VideoFormat::Hevc, _) => ("hevc", "main"),
        (VideoFormat::Mpeg2Video, _) => ("mpeg2video", "main"),
        (VideoFormat::Vc1, _) => ("vc1", "main"),
        (VideoFormat::Vp8, _) => ("vp8", "0"),
        (VideoFormat::Vp9, 10) => ("vp9", "profile 2"),
        (VideoFormat::Vp9, _) => ("vp9", "profile 0"),
        (VideoFormat::Av1, _) => ("av1", "main"),
    }
}
