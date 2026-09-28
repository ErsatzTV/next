use clap::{Parser, Subcommand};
use ffpipeline::capabilities::amf::{AmfCapabilities, AmfDeviceTarget};
use ffpipeline::capabilities::nvidia::NvidiaCapabilities;
use ffpipeline::capabilities::opencl::OpenCLCapabilities;
use ffpipeline::capabilities::qsv::QsvCapabilities;
use ffpipeline::capabilities::rkmpp::RkmppCapabilities;
use ffpipeline::capabilities::vaapi::{RateControlMode, VaapiCapabilities};
use ffpipeline::capabilities::videotoolbox::VideoToolboxCapabilities;
use ffpipeline::capabilities::vulkan::{VulkanCapabilities, format_uuid};
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
    Amf {
        /// DXGI adapter index (Windows only); default is the discrete AMD adapter
        #[arg(long)]
        adapter: Option<u32>,
    },
    Cuda,
    Qsv {
        /// Also dump the raw VPL tree and MFXVideoVPP_Query statuses
        #[arg(long)]
        raw: bool,
    },
    /// Run hwupload and vpp_qsv through ffmpeg and compare the results with what the
    /// probe predicts
    QsvVerify {
        #[arg(long, default_value = "ffmpeg")]
        ffmpeg: String,
        /// Device arguments, split on whitespace. The default matches ErsatzTV.
        #[arg(
            long,
            default_value = "-init_hw_device qsv=hw -filter_hw_device hw",
            allow_hyphen_values = true
        )]
        device_args: String,
        /// An HDR10 file with mastering metadata, to tonemap frames from the QSV decoder.
        /// Defaults to the 1080p_hevc_10_hdr.ts test fixture
        #[arg(long)]
        hdr_input: Option<String>,
    },
    Rkmpp,
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
        PixelFormat::Nv15 => "nv15",
        PixelFormat::P010le => "p010le",
        PixelFormat::Yuv420p => "yuv420p",
        PixelFormat::Yuv420p10le => "yuv420p10le",
        PixelFormat::Bgra => "bgra",
        PixelFormat::Rgba => "rgba",
        PixelFormat::Yuva420p => "yuva420p",
        PixelFormat::Yuva420p10le => "yuva420p10le",
        PixelFormat::P016 => "p016",
    }
}

fn main() {
    let cli = Cli::parse();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    let result = match cli.accel {
        Accel::Amf { adapter } => print_amf(adapter),
        Accel::Cuda => print_cuda(),
        Accel::Qsv { raw } => print_qsv(raw),
        Accel::QsvVerify {
            ffmpeg,
            device_args,
            hdr_input,
        } => qsv_verify::run(&ffmpeg, &device_args, hdr_input.as_deref()),
        Accel::Rkmpp => print_rkmpp(),
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

fn print_amf(adapter: Option<u32>) -> Result<(), String> {
    let target = adapter.map_or(AmfDeviceTarget::Auto, AmfDeviceTarget::Adapter);
    let caps = AmfCapabilities::probe_with(target).map_err(|e| e.to_string())?;
    println!("=== AMF (AMD) Capabilities ===");
    match caps.adapter() {
        Some(adapter) => println!(
            "  Adapter: {} ({:04x}:{:04x} {})",
            adapter.index, adapter.vendor_id, adapter.device_id, adapter.description
        ),
        None => println!("  Adapter: (runtime default)"),
    }
    match caps.runtime_version() {
        Some((major, minor, release, build)) => {
            println!("  Runtime: {major}.{minor}.{release}.{build}");
        }
        None => println!("  Runtime: unknown"),
    }
    println!(
        "  Device:  {}",
        caps.device().map_or("(none)", |d| d.name())
    );
    println!();

    print_decode_table(ALL_FORMATS, |f, bd| caps.can_decode(f, bd));

    println!();
    println!("Encoders:");
    println!(
        "  {:<12} {:<8} {:<8} {:<8} {:<12} {:<8}",
        "Codec", "8-bit", "10-bit", "B-Frames", "Max Profile", "Max Level"
    );
    println!(
        "  {:<12} {:<8} {:<8} {:<8} {:<12} {:<8}",
        "-----", "-----", "------", "--------", "-----------", "---------"
    );
    for f in ALL_FORMATS {
        if let Some(encoder) = caps.encoder(f) {
            println!(
                "  {:<12} {:<8} {:<8} {:<8} {:<12} {:<8}",
                format_name(f),
                yn(caps.can_encode(f, 8)),
                yn(caps.can_encode(f, 10)),
                yn(encoder.b_frames),
                encoder
                    .max_profile
                    .map_or_else(|| String::from("?"), |p| p.to_string()),
                encoder
                    .max_level
                    .map_or_else(|| String::from("?"), |l| l.to_string()),
            );
        }
    }

    println!();
    print_vpp_table(|pf| caps.vpp_supports_format(pf));

    println!();
    println!("AMFVideoConverter (vpp_amf) Surface Formats:");
    let input = caps.vpp_input_formats();
    let output = caps.vpp_output_formats();
    println!(
        "  Input:  {}",
        if input.is_empty() {
            String::from("(none)")
        } else {
            input.join(" ")
        }
    );
    println!(
        "  Output: {}",
        if output.is_empty() {
            String::from("(none)")
        } else {
            output.join(" ")
        }
    );

    Ok(())
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

    println!();
    print_cuda_vulkan_tonemap(&caps);

    Ok(())
}

/// HDR on CUDA decodes through Vulkan so libplacebo can tone map, so the answer
/// depends on what the Vulkan device backing this CUDA device can decode.
fn print_cuda_vulkan_tonemap(caps: &NvidiaCapabilities) {
    println!("Vulkan Tone Mapping (HDR):");

    let device_uuid = caps.device_uuid();
    println!(
        "  CUDA device:   {}",
        device_uuid.map_or_else(|| String::from("(uuid unavailable)"), format_uuid)
    );
    let vulkan = match VulkanCapabilities::probe_for_nvidia(device_uuid) {
        Ok(vulkan) => vulkan,
        Err(e) => {
            println!("  Vulkan:        unavailable ({e})");
            println!("  HDR falls back to NVDEC decode with software tone mapping.");
            return;
        }
    };

    println!();
    println!("  {:<12} {:<8} {:<8}", "Codec", "8-bit", "10-bit");
    println!("  {:<12} {:<8} {:<8}", "-----", "-----", "------");

    let mut any = false;
    for f in ALL_FORMATS {
        // the pipeline only reaches the Vulkan branch for codecs NVDEC also handles
        let hdr8 = caps.can_decode(f, 8) && vulkan.can_decode(f, 8);
        let hdr10 = caps.can_decode(f, 10) && vulkan.can_decode(f, 10);
        if hdr8 || hdr10 {
            println!("  {:<12} {:<8} {:<8}", format_name(f), yn(hdr8), yn(hdr10));
            any = true;
        }
    }
    if !any {
        println!("  (no codec can decode on both NVDEC and Vulkan)");
    }

    println!();
    println!("  10-bit is the column that matters; HDR sources are 10-bit.");
    println!("  Also requires ffmpeg with the vulkan hwaccel and the libplacebo filter.");
}

fn print_qsv(raw: bool) -> Result<(), String> {
    let caps = QsvCapabilities::probe().map_err(|e| e.to_string())?;
    println!("=== QSV (Intel VPL) Capabilities ===");
    println!();

    print_decode_table(ALL_FORMATS, |f, bd| caps.can_decode(f, bd));
    println!();
    print_encode_table(ALL_FORMATS, |f, bd| caps.can_encode(f, bd));

    println!();
    print_vpp_table(|pf| caps.vpp_supports_format(pf));

    println!();
    match caps.runtime_api() {
        Some((major, minor)) => println!("Runtime API: {major}.{minor}"),
        None => println!("Runtime API: unknown"),
    }
    let filters = caps.vpp_filters();
    if filters.is_empty() {
        println!("VPP Filters: (none reported; legacy Media SDK runtimes cannot list them)");
    } else {
        println!("VPP Filters: {}", filters.join(" "));
    }
    println!("Can Tonemap: {}", caps.can_tonemap());
    for pf in [PixelFormat::Nv12, PixelFormat::P010le, PixelFormat::Bgra] {
        println!(
            "Rotation {}: {}",
            pixel_format_name(&pf),
            yn(caps.can_rotate(&pf))
        );
    }
    for input in [PixelFormat::Nv12, PixelFormat::P010le] {
        for output in [PixelFormat::Nv12, PixelFormat::P010le] {
            println!(
                "Pad {} -> {}: {}",
                pixel_format_name(&input),
                pixel_format_name(&output),
                yn(caps.can_pad(&input, &output))
            );
        }
    }

    if raw {
        println!();
        print!(
            "{}",
            QsvCapabilities::diagnostics().map_err(|e| e.to_string())?
        );
    }

    Ok(())
}

fn print_rkmpp() -> Result<(), String> {
    let caps = RkmppCapabilities::probe().map_err(|e| e.to_string())?;
    println!("=== RKMPP (Rockchip MPP) Capabilities ===");
    println!();

    print_decode_table(ALL_FORMATS, |f, bd| caps.can_decode(f, bd));
    println!();
    print_encode_table(ALL_FORMATS, |f, bd| caps.can_encode(f, bd));

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

    println!();
    println!("Overlay: {}", yn(caps.can_overlay()));
    for dir in [
        ffpipeline::video_filter::TransposeDir::Clock,
        ffpipeline::video_filter::TransposeDir::CClock,
        ffpipeline::video_filter::TransposeDir::Reversal,
    ] {
        println!("Rotation {dir:?}: {}", yn(caps.can_rotate(dir)));
    }

    println!();
    println!("Rate Control:");
    println!("  {:<12} {:<8} {:<14}", "Codec", "Bits", "Forced Mode");
    println!("  {:<12} {:<8} {:<14}", "-----", "----", "-----------");
    for f in ALL_FORMATS {
        for bd in [8u8, 10] {
            let forced = caps.rate_control_mode_for(f, bd);
            if caps.can_encode(f, bd) || caps.can_encode_low_power(f, bd) {
                let label = match forced {
                    Some(RateControlMode::Cqp) => "CQP (forced)",
                    None => "(default)",
                };
                println!("  {:<12} {:<8} {:<14}", format_name(f), bd, label);
            }
        }
    }

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
        (VideoFormat::H264, 10) => ("h264", "high 10"),
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

/// Runs the filter chains ErsatzTV builds for QSV through a real ffmpeg, and puts
/// each result next to what `QsvCapabilities` predicts.
mod qsv_verify {
    use std::process::Command;

    use ffpipeline::capabilities::qsv::QsvCapabilities;
    use ffpipeline::pipeline::{PixelFormat, VideoFormat};

    use super::{pixel_format_name, yn};

    const FORMATS: [PixelFormat; 3] = [PixelFormat::Nv12, PixelFormat::P010le, PixelFormat::Bgra];
    const UPLOAD: &str = "hwupload=extra_hw_frames=16";
    /// HEVC 10-bit with HDR10 metadata, used when `--hdr-input` is not given
    const HDR10_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/1080p_hevc_10_hdr.ts"
    );

    struct Case {
        group: &'static str,
        name: String,
        predicted: bool,
        note: &'static str,
        input: Vec<String>,
        filter: String,
        /// The same chain without the operation under test. When both run and hash
        /// the same, ffmpeg skipped the operation, so the case counts as a failure.
        baseline: Option<String>,
    }

    fn lavfi(size: &str) -> Vec<String> {
        vec![
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!("testsrc2=size={size}:rate=30"),
        ]
    }

    pub fn run(ffmpeg: &str, device_args: &str, hdr_input: Option<&str>) -> Result<(), String> {
        let caps = QsvCapabilities::probe().map_err(|e| e.to_string())?;
        let version = Command::new(ffmpeg)
            .args(["-hide_banner", "-version"])
            .output()
            .map_err(|e| format!("cannot run {ffmpeg}: {e}"))?;
        let help = Command::new(ffmpeg)
            .args(["-hide_banner", "-h", "filter=vpp_qsv"])
            .output()
            .map_err(|e| format!("cannot run {ffmpeg}: {e}"))?;
        let help = String::from_utf8_lossy(&help.stdout);
        let has_pad = help.contains("pad_w");

        println!("=== QSV verify ===");
        println!(
            "ffmpeg:      {}",
            String::from_utf8_lossy(&version.stdout)
                .lines()
                .next()
                .unwrap_or("?")
        );
        println!("device args: {device_args}");
        match caps.runtime_api() {
            Some((major, minor)) => println!("runtime API: {major}.{minor}"),
            None => println!("runtime API: unknown"),
        }
        println!("vpp_qsv pad: {}", yn(has_pad));
        println!();

        let mut cases = Vec::new();
        let supported = |pf: &PixelFormat| caps.vpp_supports_format(pf);

        for pf in &FORMATS {
            let name = pixel_format_name(pf);
            cases.push(Case {
                group: "upload",
                name: name.into(),
                // Qsv::accepts_upload_format
                predicted: supported(pf),
                note: "",
                input: lavfi("1920x1080"),
                filter: format!("format={name},{UPLOAD},hwdownload,format={name}"),
                baseline: None,
            });
        }

        for from in &FORMATS {
            for to in FORMATS.iter().filter(|to| *to != from) {
                let (from, to_name) = (pixel_format_name(from), pixel_format_name(to));
                cases.push(Case {
                    group: "convert",
                    name: format!("{from} -> {to_name}"),
                    // accepts_upload_format(from) and can_convert_pixel_format(to)
                    predicted: supported(&PixelFormat::parse(from)) && supported(to),
                    note: if *to == PixelFormat::Bgra {
                        "format_filter never emits bgra"
                    } else {
                        ""
                    },
                    input: lavfi("1920x1080"),
                    filter: format!(
                        "format={from},{UPLOAD},vpp_qsv=format={to_name},hwdownload,format={to_name}"
                    ),
                    baseline: None,
                });
            }
        }

        for pf in &FORMATS {
            let name = pixel_format_name(pf);
            for (dir, dir_name) in [(1, "clock"), (2, "cclock"), (4, "reversal")] {
                cases.push(Case {
                    group: "rotate",
                    name: format!("{name} {dir_name}"),
                    predicted: supported(pf) && caps.can_rotate(pf),
                    note: "",
                    input: lavfi("1920x1080"),
                    filter: format!(
                        "format={name},{UPLOAD},vpp_qsv=transpose={dir},hwdownload,format={name}"
                    ),
                    baseline: None,
                });
            }
        }

        if has_pad {
            for from in [PixelFormat::Nv12, PixelFormat::P010le] {
                for to in [PixelFormat::Nv12, PixelFormat::P010le] {
                    let (from_name, to_name) = (pixel_format_name(&from), pixel_format_name(&to));
                    let format = if from == to {
                        String::new()
                    } else {
                        format!(":format={to_name}")
                    };
                    cases.push(Case {
                        group: "pad",
                        name: format!("{from_name} -> {to_name}"),
                        predicted: supported(&from) && caps.can_pad(&from, &to),
                        note: "",
                        input: lavfi("1440x1080"),
                        filter: format!(
                            "format={from_name},{UPLOAD},vpp_qsv=pad_w=1920:pad_h=1080:pad_x=-1:pad_y=-1:pad_color=black{format},hwdownload,format={to_name}"
                        ),
                        baseline: None,
                    });
                }
            }
        }

        // vpp_qsv tonemaps only frames that carry mastering display or content light
        // metadata, which lavfi sources cannot attach, so tonemap needs a real HDR10 file
        let hdr_input = hdr_input.unwrap_or(HDR10_FIXTURE);
        if !std::path::Path::new(hdr_input).exists() {
            println!("tonemap:     skipped, no HDR10 input at {hdr_input}");
            println!();
        }

        for to in [PixelFormat::Nv12, PixelFormat::P010le] {
            let to_name = pixel_format_name(&to);
            if std::path::Path::new(hdr_input).exists() {
                cases.push(Case {
                    group: "tonemap",
                    name: format!("decoded hdr -> {to_name}"),
                    predicted: caps.can_decode(&VideoFormat::Hevc, 10) && caps.can_tonemap(),
                    note: "",
                    input: vec![
                        "-hwaccel".into(),
                        "qsv".into(),
                        "-hwaccel_output_format".into(),
                        "qsv".into(),
                        "-i".into(),
                        hdr_input.into(),
                    ],
                    filter: format!(
                        "vpp_qsv=tonemap=1:format={to_name},hwdownload,format={to_name}"
                    ),
                    baseline: Some(format!(
                        "vpp_qsv=format={to_name},hwdownload,format={to_name}"
                    )),
                });
            }
        }

        println!(
            "{:<8} {:<26} {:<10} {:<8}",
            "group", "case", "predicted", "ffmpeg"
        );
        println!(
            "{:<8} {:<26} {:<10} {:<8}",
            "-----", "----", "---------", "------"
        );

        let mut mismatches = 0;
        for case in &cases {
            let (args, result) = run_ffmpeg(ffmpeg, device_args, &case.input, &case.filter);
            let (mut passed, mut error) = match &result {
                Ok(_) => (true, String::new()),
                Err(error) => (false, error.clone()),
            };

            if let (Ok(hash), Some(baseline)) = (&result, &case.baseline)
                && let (_, Ok(baseline_hash)) =
                    run_ffmpeg(ffmpeg, device_args, &case.input, baseline)
                && *hash == baseline_hash
            {
                passed = false;
                error = String::from("ran, but the output matches the chain without it (skipped)");
            }

            let mismatch = passed != case.predicted;
            if mismatch {
                mismatches += 1;
            }
            let flag = if mismatch { "MISMATCH" } else { "" };
            println!(
                "{:<8} {:<26} {:<10} {:<8} {} {}",
                case.group,
                case.name,
                yn(case.predicted),
                yn(passed),
                flag,
                case.note
            );
            if mismatch || !passed {
                if !error.is_empty() {
                    println!("         ffmpeg: {error}");
                }
                if mismatch {
                    println!("         cmd: {ffmpeg} {}", args.join(" "));
                }
            }
        }

        println!();
        println!("{mismatches} mismatches in {} cases", cases.len());
        Ok(())
    }

    /// Returns the arguments, and the md5 of the output or the first error line.
    fn run_ffmpeg(
        ffmpeg: &str,
        device_args: &str,
        input: &[String],
        filter: &str,
    ) -> (Vec<String>, Result<String, String>) {
        let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-v", "error"]
            .into_iter()
            .map(String::from)
            .collect();
        args.extend(device_args.split_whitespace().map(String::from));
        args.extend(input.iter().cloned());
        args.extend(
            ["-frames:v", "10", "-vf", filter, "-f", "md5", "-"]
                .into_iter()
                .map(String::from),
        );

        let result = match Command::new(ffmpeg).args(&args).output() {
            Ok(output) if output.status.success() => {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            }
            Ok(output) => Err(String::from_utf8_lossy(&output.stderr)
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("exited with an error")
                .to_string()),
            Err(e) => Err(e.to_string()),
        };

        (args, result)
    }
}
