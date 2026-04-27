use std::borrow::Cow;

use crate::ArgVec;

pub enum LogLevel {
    Error,
}

impl LogLevel {
    fn as_arg(&self) -> Vec<Cow<'static, str>> {
        match self {
            LogLevel::Error => args!["-loglevel", "error"],
        }
    }
}

pub enum GlobalOption {
    Threads(u32),
    NoStdIn,
    HideBanner,
    LogLevel(LogLevel),
    StandardFormatFlags,
    InitHwDevice(ArgVec),
}

impl GlobalOption {
    pub(crate) fn as_arg(&self) -> Vec<Cow<'static, str>> {
        match self {
            GlobalOption::Threads(count) => args!["-threads", count.to_string()],
            GlobalOption::NoStdIn => args!["-nostdin"],
            GlobalOption::HideBanner => args!["-hide_banner"],
            GlobalOption::LogLevel(level) => level.as_arg(),
            GlobalOption::StandardFormatFlags => args!["-fflags", "+genpts+discardcorrupt+igndts",],
            GlobalOption::InitHwDevice(args) => args.clone(),
        }
    }
}
