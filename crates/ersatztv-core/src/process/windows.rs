use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle};

use windows_sys::Win32::Foundation::{FILETIME, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

pub(super) fn kill_descendants_on_exit() {
    if let Err(err) = join_kill_on_close_job() {
        log::warn!("failed to create job object; children may outlive this process: {err}");
    }
}

fn join_kill_on_close_job() -> io::Result<()> {
    // SAFETY: plain win32 calls on handles we own
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(job);

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&raw const info).cast(),
            size_of_val(&info) as u32,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }

        if AssignProcessToJobObject(job.as_raw_handle(), GetCurrentProcess()) == 0 {
            return Err(io::Error::last_os_error());
        }

        // the OS closes it when we die, and that close kills the job
        let _ = job.into_raw_handle();
    }

    Ok(())
}

pub(super) struct Parent(OwnedHandle);

impl Parent {
    pub(super) fn current() -> Option<Self> {
        let pid = parent_pid()?;

        // SAFETY: plain win32 calls on handles we own
        unsafe {
            let handle = OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            );
            if handle.is_null() {
                return None;
            }
            let handle = OwnedHandle::from_raw_handle(handle);

            // the parent pid is never cleared, so a dead parent's pid can belong to anyone
            if creation_time(handle.as_raw_handle())? > creation_time(GetCurrentProcess())? {
                return None;
            }

            Some(Self(handle))
        }
    }

    pub(super) fn exited(&self) -> bool {
        // SAFETY: the handle is open for our lifetime
        unsafe { WaitForSingleObject(self.0.as_raw_handle(), 0) == WAIT_OBJECT_0 }
    }
}

fn parent_pid() -> Option<u32> {
    let pid = std::process::id();

    // SAFETY: plain win32 calls on handles we own
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let snapshot = OwnedHandle::from_raw_handle(snapshot);

        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = Process32FirstW(snapshot.as_raw_handle(), &mut entry) != 0;
        while found {
            if entry.th32ProcessID == pid {
                return Some(entry.th32ParentProcessID);
            }
            found = Process32NextW(snapshot.as_raw_handle(), &mut entry) != 0;
        }
    }

    None
}

unsafe fn creation_time(process: HANDLE) -> Option<u64> {
    let mut times = [FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    }; 4];
    let [creation, exit, kernel, user] = &mut times;

    // SAFETY: the caller passes an open process handle
    let ok = unsafe { GetProcessTimes(process, creation, exit, kernel, user) };
    (ok != 0)
        .then(|| (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    use super::super::parent_exit;
    use super::kill_descendants_on_exit;

    const ROLE_ENV: &str = "ETV_TEST_WINDOWS_ROLE";
    const PREFIX: &str = "process::windows::tests::";

    // the tests below run these in subprocesses; no-op otherwise
    #[test]
    fn role() {
        match std::env::var(ROLE_ENV).as_deref() {
            Ok("job") => {
                kill_descendants_on_exit();
                let mut child = Command::new("ping")
                    .args(["-n", "30", "127.0.0.1"])
                    .stdout(Stdio::null())
                    .spawn()
                    .unwrap();
                println!("child pid {}", child.id());
                let _ = child.wait();
            }
            Ok("watcher-parent") => {
                // std spawn, no job: the watcher must outlive us
                let mut watcher = self_with_role("watcher")
                    .stdout(Stdio::inherit())
                    .spawn()
                    .unwrap();
                let _ = watcher.wait();
            }
            Ok("watcher") => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()
                    .unwrap();
                runtime.block_on(async {
                    let parent_exit = parent_exit();
                    println!("watching");
                    if tokio::time::timeout(Duration::from_secs(10), parent_exit)
                        .await
                        .is_ok()
                    {
                        println!("parent exited");
                    }
                });
            }
            _ => {}
        }
    }

    fn self_with_role(role: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", &format!("{PREFIX}role"), "--nocapture"])
            .env(ROLE_ENV, role);
        command
    }

    #[test]
    fn job_kills_child_of_terminated_process() {
        let mut intermediate = self_with_role("job")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let child_pid = BufReader::new(intermediate.stdout.take().unwrap())
            .lines()
            .map_while(Result::ok)
            .find_map(|line| line.strip_prefix("child pid ")?.parse::<u32>().ok())
            .unwrap();

        // SAFETY: plain win32 call; the handle is owned below
        let child = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, child_pid) };
        assert!(!child.is_null(), "child {child_pid} not running");
        // SAFETY: OpenProcess returned an open handle
        let child = unsafe { OwnedHandle::from_raw_handle(child) };

        intermediate.kill().unwrap();
        intermediate.wait().unwrap();

        // SAFETY: the handle is open
        let wait = unsafe { WaitForSingleObject(child.as_raw_handle(), 2000) };
        assert_ne!(wait, WAIT_TIMEOUT, "child {child_pid} outlived its parent");
        assert_eq!(wait, WAIT_OBJECT_0);
    }

    #[test]
    fn parent_exit_resolves_when_parent_terminated() {
        let mut parent = self_with_role("watcher-parent")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(parent.stdout.take().unwrap())
            .lines()
            .map_while(Result::ok);
        assert!(lines.any(|line| line == "watching"));

        let start = Instant::now();
        parent.kill().unwrap();
        parent.wait().unwrap();

        // to EOF, so the watcher never writes to a closed pipe
        let rest: Vec<String> = lines.collect();
        assert!(
            rest.iter().any(|line| line == "parent exited"),
            "watcher didn't notice its parent exit"
        );
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
