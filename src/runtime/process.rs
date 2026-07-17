//! Safe subprocess launching shared by all runtime backends.
//!
//! Everything here goes through `std::process::Command` with an explicit
//! argv - never a shell string - so there is no command-injection surface
//! regardless of what ends up in a model path or prompt. Timeouts are
//! enforced by polling rather than relying on the child to behave, and
//! stdout/stderr are drained on background threads so a chatty process can
//! never deadlock us by filling a pipe buffer.

use crate::errors::ProcessError;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ProcessRun {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub wall_time: Duration,
    /// Peak working set of the child process, read via a direct Win32 API
    /// call (`GetProcessMemoryInfo`) at the moment it exits/is killed -
    /// this is the OS's own peak accounting, not a sampled approximation.
    pub peak_working_set_bytes: Option<u64>,
}

impl ProcessRun {
    pub fn succeeded(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }
}

const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Spawns `binary` with `args`, waits up to `timeout`, and kills it on
/// expiry. `on_tick` is invoked on every poll iteration (roughly every
/// `POLL_INTERVAL`) so a caller can sample metrics like system RAM while
/// the process is running.
pub fn run(
    binary: &Path,
    args: &[String],
    timeout: Duration,
    mut on_tick: impl FnMut(&Child),
) -> Result<ProcessRun, ProcessError> {
    if !binary.is_file() {
        return Err(ProcessError::BinaryNotFound(binary.to_path_buf()));
    }

    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| ProcessError::SpawnFailed {
            program: binary.to_path_buf(),
            source,
        })?;

    let stdout_handle = child.stdout.take().expect("stdout was piped");
    let stderr_handle = child.stderr.take().expect("stderr was piped");
    let stdout_thread = thread::spawn(move || drain(stdout_handle));
    let stderr_thread = thread::spawn(move || drain(stderr_handle));

    let start = Instant::now();
    let mut timed_out = false;

    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    child.kill().map_err(ProcessError::KillFailed)?;
                    let _ = child.wait();
                    break None;
                }
                on_tick(&child);
                thread::sleep(POLL_INTERVAL);
            }
            Err(source) => {
                return Err(ProcessError::SpawnFailed {
                    program: binary.to_path_buf(),
                    source,
                });
            }
        }
    };

    let wall_time = start.elapsed();
    let peak_working_set_bytes = query_peak_working_set(&child);

    let stdout_bytes = stdout_thread.join().unwrap_or_default();
    let stderr_bytes = stderr_thread.join().unwrap_or_default();

    Ok(ProcessRun {
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        exit_code: exit_status.and_then(|s| s.code()),
        timed_out,
        wall_time,
        peak_working_set_bytes,
    })
}

fn drain(mut reader: impl Read) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = reader.read_to_end(&mut buf);
    buf
}

fn query_peak_working_set(child: &Child) -> Option<u64> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};

    let handle = HANDLE(child.as_raw_handle());
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };

    unsafe { GetProcessMemoryInfo(handle, &mut counters, counters.cb) }
        .ok()
        .map(|()| counters.PeakWorkingSetSize as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd_exe() -> std::path::PathBuf {
        Path::new(r"C:\Windows\System32\cmd.exe").to_path_buf()
    }

    #[test]
    fn runs_a_real_process_and_captures_stdout() {
        let result = run(
            &cmd_exe(),
            &["/C".to_string(), "echo hello-from-brute".to_string()],
            Duration::from_secs(10),
            |_| {},
        )
        .expect("cmd.exe should run");

        assert!(result.stdout.contains("hello-from-brute"));
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.timed_out);
    }

    #[test]
    fn reports_nonzero_exit_code() {
        let result = run(
            &cmd_exe(),
            &["/C".to_string(), "exit 7".to_string()],
            Duration::from_secs(10),
            |_| {},
        )
        .expect("cmd.exe should run");

        assert_eq!(result.exit_code, Some(7));
    }

    #[test]
    fn kills_process_on_timeout() {
        // `timeout.exe` refuses to run with redirected stdin, so use `ping`
        // as a stdin-independent way to occupy the process for ~29s.
        let result = run(
            &cmd_exe(),
            &["/C".to_string(), "ping -n 30 127.0.0.1 >NUL".to_string()],
            Duration::from_millis(500),
            |_| {},
        )
        .expect("cmd.exe should run");

        assert!(result.timed_out);
        assert!(result.wall_time < Duration::from_secs(5));
    }

    #[test]
    fn missing_binary_is_a_clear_error() {
        let result = run(
            Path::new(r"C:\nonexistent\brute-test-binary.exe"),
            &[],
            Duration::from_secs(1),
            |_| {},
        );
        assert!(matches!(result, Err(ProcessError::BinaryNotFound(_))));
    }

    #[test]
    fn on_tick_is_called_for_a_slow_process() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let ticks = Arc::new(AtomicUsize::new(0));
        let ticks_clone = ticks.clone();

        let _ = run(
            &cmd_exe(),
            &["/C".to_string(), "ping -n 3 127.0.0.1 >NUL".to_string()],
            Duration::from_secs(5),
            move |_| {
                ticks_clone.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert!(ticks.load(Ordering::SeqCst) > 0);
    }
}
