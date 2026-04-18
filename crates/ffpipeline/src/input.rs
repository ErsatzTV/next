use std::time::Duration;

use enum_dispatch::enum_dispatch;
use simple_expand_tilde::expand_tilde;

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
pub struct LocalInputSource {
    pub path: String,
}

impl LocalInputSource {
    pub fn expand_path(&self) -> Option<String> {
        let expanded_path_buf = expand_tilde(self.path.as_str()); //.ok_or(ChannelError::PlayoutJsonInvalidLocalSource)?;
        expanded_path_buf
            .map(|p| p.into_os_string())
            .and_then(|p| p.into_string().ok())
    }
}

#[derive(Clone)]
pub struct LavfiInputSource {
    pub params: String,
}

#[derive(Clone)]
pub struct HttpInputSource {
    pub uri: String,
    pub options: HttpInputOptions,
}

#[derive(Clone)]
#[enum_dispatch(Probeable)]
#[enum_dispatch(FfmpegInputArgs)]
pub enum InputSource {
    Local(LocalInputSource),
    Lavfi(LavfiInputSource),
    Http(HttpInputSource),
}

#[enum_dispatch]
pub trait FfmpegInputArgs {
    fn args_for_input(&self) -> Vec<String>;
}

impl FfmpegInputArgs for LocalInputSource {
    fn args_for_input(&self) -> Vec<String> {
        [].to_vec()
    }
}

impl FfmpegInputArgs for LavfiInputSource {
    fn args_for_input(&self) -> Vec<String> {
        vec!["-f".to_string(), "lavfi".to_string()]
    }
}
impl FfmpegInputArgs for HttpInputSource {
    fn args_for_input(&self) -> Vec<String> {
        let mut args = Vec::new();

        if self.options.reconnect {
            args.extend([
                String::from("-reconnect"),
                String::from("1"),
                String::from("-reconnect_on_network_error"),
                String::from("1"),
                String::from("-reconnect_streamed"),
                String::from("1"),
                String::from("-multiple_requests"),
                String::from("1"),
            ]);
            if let Some(max_delay) = self.options.reconnect_delay_max {
                args.extend([String::from("-reconnect_delay_max"), max_delay.to_string()]);
            }
        }

        if let Some(timeout) = self.options.timeout_us {
            args.extend([String::from("-timeout"), timeout.to_string()]);
        }

        if let Some(ua) = &self.options.user_agent {
            args.extend([String::from("-user_agent"), ua.clone()]);
        }

        if !self.options.headers.is_empty() {
            // FFmpeg expects headers separated by \r\n, with trailing \r\n
            let combined: String = self
                .options
                .headers
                .iter()
                .map(|h| format!("{}\r\n", h))
                .collect();
            args.extend([String::from("-headers"), combined]);
        }

        args.extend([
            String::from("-protocol_whitelist"),
            String::from("file,http,https,tcp,tls,crypto"),
        ]);

        args
    }
}

pub struct ProbedInput {
    pub input_source: InputSource,
    pub probe_result: ProbeResult,
    pub in_point: Duration,
    pub out_point: Duration,
    pub audio_index: Option<u32>,
    pub video_index: Option<u32>,
}
