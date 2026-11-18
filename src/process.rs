//! Bounded subprocess execution with inherited environment and no shell expansion.
use std::{
    io::{ErrorKind, Read},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};
const OUTPUT_LIMIT: usize = 1_048_576;

/// Own the isolated group until termination, and always reap the direct child.
/// Init reaps orphan grandchildren; a descendant deliberately leaving the group
/// is outside this boundary. No process-wide subreaper or reader threads are used.
struct SourceProcess {
    child: Child,
    terminated: bool,
    status: Option<ExitStatus>,
}

impl SourceProcess {
    fn terminate(&mut self) {
        if !self.terminated {
            // process_group(0) made the child PID its own PGID. Kill the group
            // before reaping the leader so its PID cannot be reused meanwhile.
            unsafe {
                libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = self.child.kill();
            self.terminated = true;
        }
    }

    fn status(&mut self) -> Result<Option<ExitStatus>, String> {
        if self.status.is_none() {
            // Observe exit without reaping; keep the group leader PID reserved
            // until group termination. try_wait() would reap too early.
            let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.child.id(),
                    info.as_mut_ptr(),
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result != 0 {
                if std::io::Error::last_os_error().kind() == ErrorKind::Interrupted {
                    return Ok(None);
                }
                return Err("Cannot observe data source process".into());
            }
            // waitid initializes siginfo; a zero PID means the child is running.
            if unsafe { info.assume_init().si_pid() } != 0 {
                self.terminate();
                self.status = Some(
                    self.child
                        .wait()
                        .map_err(|_| "Cannot reap data source process")?,
                );
            }
        }
        Ok(self.status)
    }
}

impl Drop for SourceProcess {
    fn drop(&mut self) {
        self.terminate();
        let _ = self.child.wait();
    }
}

pub fn output(command: &mut Command, timeout: Duration) -> Result<String, String> {
    let start = Instant::now();
    let child = command
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Cannot start data source process")?;
    let mut process = SourceProcess {
        child,
        terminated: false,
        status: None,
    };
    let mut stdout = process
        .child
        .stdout
        .take()
        .ok_or("Missing process output")?;
    let fd = stdout.as_raw_fd();
    // stdout owns a valid pipe FD. Nonblocking reads let the same deadline govern
    // both process execution and descendants holding the pipe after leader exit.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err("Cannot configure data source output pipe".into());
    }
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        if start.elapsed() >= timeout {
            return Err("Data source timed out".into());
        }
        let status = process.status()?;
        if status.is_some_and(|s| !s.success()) {
            return Err("Data source failed (network, permission or rate limit); retry at next scheduled refresh".into());
        }
        match stdout.read(&mut buffer) {
            Ok(n) if n > 0 => {
                if n > OUTPUT_LIMIT - bytes.len() {
                    return Err("Data source output exceeds 1 MiB".into());
                }
                bytes.extend_from_slice(&buffer[..n]);
                continue;
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Err("Cannot read data source output".into()),
        }
        if status.is_some() {
            // The group has been stopped. Keep all buffered successful output,
            // then close the pipe rather than wait for an escaped pipe holder.
            return String::from_utf8(bytes).map_err(|_| "Data source output is not UTF-8".into());
        }
        std::thread::sleep(Duration::from_millis(20).min(timeout.saturating_sub(start.elapsed())));
    }
}

fn curl_command(url: &str) -> Command {
    let mut command = Command::new("curl");
    command.args([
        "--disable", // Must be first: never load ambient ~/.curlrc options/credentials.
        "--fail",
        "--silent",
        "--show-error",
        "--connect-timeout",
        "10",
        "--max-time",
        "20",
        "--max-filesize",
        "1048576",
        "--proto",
        "=https",
        url,
    ]);
    command
}

pub fn json_url(url: &str) -> Result<String, String> {
    output(&mut curl_command(url), Duration::from_secs(25))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "stripe-process-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn pid(&self, name: &str) -> i32 {
            std::fs::read_to_string(self.0.join(name))
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        }
        fn wrapper(&self, tail: &str) -> Command {
            let mut command = Command::new("sh");
            command.arg("-c").arg(format!("echo $$ > \"$1/child\"; sleep 30 & echo $! > \"$1/descendant\"; printf synthetic; {tail}"))
                .arg("synthetic-wrapper").arg(&self.0);
            command
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Clean up intentionally failing pre-fix tests as well.
            for name in ["descendant", "child"] {
                if let Ok(text) = std::fs::read_to_string(self.0.join(name))
                    && let Ok(pid) = text.trim().parse::<i32>()
                {
                    unsafe {
                        libc::kill(pid, libc::SIGKILL);
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn running(pid: i32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .is_some_and(|stat| {
                stat.rsplit_once(") ")
                    .is_some_and(|(_, fields)| !fields.starts_with('Z'))
            })
    }
    fn assert_stopped_and_reaped(fixture: &Fixture) {
        let descendant = fixture.pid("descendant");
        let until = Instant::now() + Duration::from_millis(500);
        while running(descendant) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !running(descendant),
            "descendant still executing/holding stdout"
        );
        let mut status = 0;
        assert_eq!(
            unsafe { libc::waitpid(fixture.pid("child"), &mut status, libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }

    #[test]
    fn curl_ignores_synthetic_ambient_config_without_network() {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join(".curlrc"),"header = \"Authorization: SYNTHETIC-NOT-A-CREDENTIAL\"\nurl = \"file:///synthetic-unused\"\nsynthetic-invalid-option = true\n").unwrap();
        let mut command = curl_command("https://synthetic-unused.invalid/");
        let first = command.get_args().next().unwrap().to_owned();
        // --version performs no transfer even when the ambient config adds URLs.
        let result = command
            .arg("--version")
            .env("CURL_HOME", &fixture.0)
            .env("HOME", &fixture.0)
            .env("XDG_CONFIG_HOME", &fixture.0)
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(
            result.stderr.is_empty(),
            "ambient .curlrc was read: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(first, "--disable");
        assert!(String::from_utf8_lossy(&result.stdout).starts_with("curl "));
    }

    #[test]
    fn timeout_stops_descendant_and_closes_reader() {
        let fixture = Fixture::new();
        let start = Instant::now();
        let result = output(&mut fixture.wrapper("wait"), Duration::from_millis(100));
        assert!(result.unwrap_err().contains("timed out"));
        assert!(start.elapsed() < Duration::from_millis(500));
        assert_stopped_and_reaped(&fixture);
    }

    #[test]
    fn successful_child_exit_stops_pipe_holding_descendant_and_keeps_output() {
        let fixture = Fixture::new();
        let start = Instant::now();
        let result = output(&mut fixture.wrapper("exit 0"), Duration::from_millis(100));
        assert_stopped_and_reaped(&fixture);
        assert_eq!(result.unwrap(), "synthetic");
        assert!(start.elapsed() < Duration::from_millis(800));
    }

    #[test]
    fn failure_and_output_limit_also_stop_descendants() {
        for tail in ["exit 1", "head -c 1048577 /dev/zero; wait"] {
            let fixture = Fixture::new();
            let result = output(&mut fixture.wrapper(tail), Duration::from_secs(2));
            let error = result.unwrap_err();
            assert!(error.contains(if tail == "exit 1" {
                "failed"
            } else {
                "exceeds 1 MiB"
            }));
            assert_stopped_and_reaped(&fixture);
        }
    }

    #[test]
    fn process_success_failure_size_and_timeout_are_bounded() {
        assert_eq!(
            output(
                Command::new("printf").arg("synthetic"),
                Duration::from_secs(1)
            )
            .unwrap(),
            "synthetic"
        );
        assert!(output(&mut Command::new("false"), Duration::from_secs(1)).is_err());
        assert_eq!(
            output(&mut Command::new("true"), Duration::from_secs(1)).unwrap(),
            ""
        );
        assert_eq!(
            output(
                Command::new("head").args(["-c", "1048576", "/dev/zero"]),
                Duration::from_secs(2)
            )
            .unwrap()
            .len(),
            OUTPUT_LIMIT
        );
        assert!(
            output(Command::new("printf").arg("\\377"), Duration::from_secs(1))
                .unwrap_err()
                .contains("not UTF-8")
        );
        assert_eq!(
            output(
                Command::new("printenv")
                    .arg("SIDESCOPE_SYNTHETIC_ENV")
                    .env("SIDESCOPE_SYNTHETIC_ENV", "inherited"),
                Duration::from_secs(1)
            )
            .unwrap(),
            "inherited\n"
        );
        let start = Instant::now();
        assert!(
            output(Command::new("sleep").arg("3"), Duration::from_millis(50))
                .unwrap_err()
                .contains("timed out")
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(
            output(
                Command::new("head").args(["-c", "1048577", "/dev/zero"]),
                Duration::from_secs(2)
            )
            .is_err()
        );
    }
}
