use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;

const FORMAT_ENV: &str = "ETV_LOG_FORMAT";

const DEFAULT_FILTER: &str = "info,ersatztv_channel=debug,ersatztv_core=debug,\
    ersatztv_playout=debug,ffpipeline=debug,ffmpeg=debug";

pub fn init() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);

    if std::env::var(FORMAT_ENV).is_ok_and(|v| v.eq_ignore_ascii_case("json")) {
        builder
            .json()
            .flatten_event(true)
            .with_span_list(false)
            .init();
    } else {
        builder.with_ansi(std::io::stderr().is_terminal()).init();
    }
}
