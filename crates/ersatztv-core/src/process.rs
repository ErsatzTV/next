use std::ffi::OsStr;

use tokio::process::Command;

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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    use super::command;

    const INTERMEDIATE_ENV: &str = "ETV_TEST_PDEATHSIG_INTERMEDIATE";

    // re-executed by child_dies_with_killed_parent; a no-op in a normal test run
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
                "process::tests::pdeathsig_intermediate",
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

    // a killed child stays a zombie until its new parent reaps it
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
