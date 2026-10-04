use std::ffi::OsStr;
use std::process::ExitStatus;
use std::time::Duration;

use tokio::process::{Child, Command};

#[cfg(windows)]
mod windows;

/// A command whose child dies with this process: on drop, and on Linux also when this process is
/// killed. Spawn only from async context; Linux ties the death signal to the spawning thread, and
/// blocking-pool threads exit when idle.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(clippy::disallowed_methods)]
    let mut command = Command::new(program);
    command.kill_on_drop(true);

    #[cfg(target_os = "linux")]
    set_parent_death_signal(&mut command);

    command
}

pub async fn stop(child: &mut Child, deadline: Duration) -> std::io::Result<ExitStatus> {
    // no SIGTERM on Windows; console Ctrl+C already reaches the child
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: an unreaped pid can't be reused
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }

    match tokio::time::timeout(deadline, child.wait()).await {
        Ok(status) => status,
        Err(_) => {
            child.kill().await?;
            child.wait().await
        }
    }
}

/// Reads the parent at call time, not first poll. Never resolves if the parent is init or already
/// gone.
pub fn parent_exit() -> impl Future<Output = ()> + Send + 'static {
    let parent = Parent::current();

    async move {
        if let Some(parent) = parent {
            let mut interval = tokio::time::interval(PARENT_POLL_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                if parent.exited() {
                    return;
                }
            }
        }

        std::future::pending().await
    }
}

const PARENT_POLL_INTERVAL: Duration = Duration::from_secs(1);

// not PDEATHSIG: it is per spawning thread, and .NET spawns from pool threads
#[cfg(unix)]
struct Parent(u32);

#[cfg(unix)]
impl Parent {
    fn current() -> Option<Self> {
        let pid = std::os::unix::process::parent_id();
        (pid > 1).then_some(Self(pid))
    }

    fn exited(&self) -> bool {
        std::os::unix::process::parent_id() != self.0
    }
}

#[cfg(windows)]
use windows::Parent;

/// Windows only: every descendant dies when this process exits, however it exits. Linux gets this
/// per child from [`command`]; macOS has no equivalent.
pub fn kill_descendants_on_exit() {
    #[cfg(windows)]
    windows::kill_descendants_on_exit();
}

#[cfg(unix)]
pub async fn shutdown_signal() -> &'static str {
    use tokio::signal::unix::{SignalKind, signal};

    let (Ok(mut terminate), Ok(mut interrupt)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) else {
        log::warn!("failed to install signal handlers");
        return std::future::pending().await;
    };

    tokio::select! {
        _ = terminate.recv() => "SIGTERM",
        _ = interrupt.recv() => "SIGINT",
    }
}

#[cfg(not(unix))]
pub async fn shutdown_signal() -> &'static str {
    if tokio::signal::ctrl_c().await.is_err() {
        log::warn!("failed to install ctrl+c handler");
        return std::future::pending().await;
    }

    "ctrl+c"
}

#[cfg(target_os = "linux")]
fn set_parent_death_signal(command: &mut Command) {
    let parent = std::process::id() as libc::pid_t;

    // SAFETY: only async-signal-safe calls, and no allocation, between fork and exec
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == -1 {
                return Err(std::io::Error::last_os_error());
            }

            // the parent died before prctl, so the signal will never come
            if libc::getppid() != parent {
                libc::_exit(1);
            }

            Ok(())
        });
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    use tokio::io::AsyncBufReadExt;

    use super::{command, parent_exit, stop};

    #[tokio::test]
    async fn stop_sends_sigterm() {
        let mut child = command("sleep").arg("30").spawn().unwrap();
        let status = stop(&mut child, Duration::from_secs(5)).await.unwrap();
        assert_eq!(status.signal(), Some(libc::SIGTERM));
    }

    #[tokio::test]
    async fn stop_kills_after_deadline() {
        let mut child = command("sh")
            .args(["-c", "trap '' TERM; echo ready; exec sleep 30"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        // a SIGTERM before the trap ends sh
        let stdout = child.stdout.take().unwrap();
        tokio::io::BufReader::new(stdout)
            .lines()
            .next_line()
            .await
            .unwrap();

        let start = Instant::now();
        let status = stop(&mut child, Duration::from_millis(200)).await.unwrap();
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    const WATCHER_ENV: &str = "ETV_TEST_PARENT_EXIT_WATCHER";

    // parent_exit_resolves_when_reparented runs this in a subprocess; no-op otherwise
    #[tokio::test]
    async fn parent_exit_watcher() {
        if std::env::var_os(WATCHER_ENV).is_none() {
            return;
        }

        let parent_exit = parent_exit();
        println!("watching");
        if tokio::time::timeout(Duration::from_secs(10), parent_exit)
            .await
            .is_ok()
        {
            println!("parent exited");
        }
    }

    #[test]
    fn parent_exit_resolves_when_reparented() {
        use std::io::{BufRead, BufReader};

        // std, not the helper, so the watcher outlives sh; exec keeps sh's pid
        let mut parent = std::process::Command::new("sh")
            .args([
                "-c",
                "\"$0\" --exact process::tests::parent_exit_watcher --nocapture & exec sleep 30",
            ])
            .arg(std::env::current_exe().unwrap())
            .env(WATCHER_ENV, "1")
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

    #[cfg(target_os = "linux")]
    mod parent_death {
        use std::io::{BufRead, BufReader};
        use std::process::Stdio;
        use std::time::{Duration, Instant};

        use super::super::command;

        const INTERMEDIATE_ENV: &str = "ETV_TEST_PDEATHSIG_INTERMEDIATE";

        // child_dies_with_killed_parent runs this in a subprocess; no-op otherwise
        #[tokio::test]
        async fn pdeathsig_intermediate() {
            if std::env::var_os(INTERMEDIATE_ENV).is_none() {
                return;
            }

            let child = command("sleep").arg("30").spawn().unwrap();
            println!("child pid {}", child.id().unwrap());
            tokio::time::sleep(Duration::from_secs(30)).await;
        }

        #[test]
        fn child_dies_with_killed_parent() {
            let mut intermediate = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process::tests::parent_death::pdeathsig_intermediate",
                    "--nocapture",
                ])
                .env(INTERMEDIATE_ENV, "1")
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();

            let stdout = BufReader::new(intermediate.stdout.take().unwrap());
            let child_pid = stdout
                .lines()
                .map_while(Result::ok)
                .find_map(|line| line.strip_prefix("child pid ")?.parse::<u32>().ok())
                .unwrap();

            intermediate.kill().unwrap();
            intermediate.wait().unwrap();

            let deadline = Instant::now() + Duration::from_secs(2);
            while is_running(child_pid) {
                assert!(
                    Instant::now() < deadline,
                    "child {child_pid} outlived its parent"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        // a zombie is dead but not yet reaped
        fn is_running(pid: u32) -> bool {
            std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|stat| {
                    let state = stat.rsplit_once(") ")?.1.chars().next()?;
                    Some(state != 'Z')
                })
                .unwrap_or(false)
        }
    }
}
