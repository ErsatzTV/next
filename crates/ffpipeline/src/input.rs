use std::time::Duration;

use crate::probe::ProbeResult;

pub struct InputSettings {
    pub audio_input: ProbedInput,
    pub video_input: ProbedInput,
}

#[derive(Clone, Debug)]
pub struct HttpInputOptions {
    pub headers: Vec<String>,
    pub user_agent: Option<String>,
    pub timeout_us: Option<u64>,
    pub reconnect: bool,
    pub reconnect_delay_max: Option<u32>,
}

#[derive(Clone)]
pub enum InputSource {
    Local {
        path: String,
    },
    Lavfi {
        params: String,
    },
    Http {
        uri: String,
        options: HttpInputOptions,
    },
}

pub struct ProbedInput {
    pub input_source: InputSource,
    pub probe_result: ProbeResult,
    pub in_point: Duration,
    pub out_point: Duration,
    pub audio_index: Option<u32>,
    pub video_index: Option<u32>,
}
