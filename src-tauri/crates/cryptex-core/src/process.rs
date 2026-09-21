use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;

const READ_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub termination_grace: Duration,
    pub max_log_bytes: usize,
    pub max_address_space_bytes: u64,
    pub max_file_bytes: u64,
    pub max_open_files: u64,
    pub cpu_seconds: u64,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            termination_grace: Duration::from_millis(250),
            max_log_bytes: 4 * 1024 * 1024,
            max_address_space_bytes: 2 * 1024 * 1024 * 1024,
            max_file_bytes: 512 * 1024 * 1024,
            max_open_files: 256,
            cpu_seconds: 60,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputChunk {
    pub stream: OutputStream,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessOutcome {
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessResult {
    pub outcome: ProcessOutcome,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub log_truncated: bool,
    pub elapsed: Duration,
}

pub struct ProcessSupervisor {
    executables: BTreeMap<String, PathBuf>,
    working_roots: Vec<PathBuf>,
    limits: ProcessLimits,
}

impl ProcessSupervisor {
    pub fn new(
        executables: BTreeMap<String, PathBuf>,
        working_roots: Vec<PathBuf>,
        limits: ProcessLimits,
    ) -> Result<Self, ProcessError> {
        if executables.is_empty() || working_roots.is_empty() {
            return Err(ProcessError::InvalidConfiguration);
        }
        let executables = executables
            .into_iter()
            .map(|(name, path)| {
                validate_name(&name)?;
                let path = path.canonicalize().map_err(ProcessError::Io)?;
                if !path.is_file() {
                    return Err(ProcessError::InvalidExecutable);
                }
                Ok((name, path))
            })
            .collect::<Result<_, _>>()?;
        let working_roots = working_roots
            .into_iter()
            .map(|path| path.canonicalize().map_err(ProcessError::Io))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            executables,
            working_roots,
            limits,
        })
    }

    pub fn run<F>(
        &self,
        executable_name: &str,
        arguments: &[OsString],
        working_directory: &Path,
        cancellation: &CancellationToken,
        mut on_output: F,
    ) -> Result<ProcessResult, ProcessError>
    where
        F: FnMut(OutputChunk),
    {
        let executable = self
            .executables
            .get(executable_name)
            .ok_or(ProcessError::ExecutableNotAllowed)?;
        let working_directory = working_directory.canonicalize().map_err(ProcessError::Io)?;
        if !working_directory.is_dir()
            || !self
                .working_roots
                .iter()
                .any(|root| working_directory.starts_with(root))
        {
            return Err(ProcessError::WorkingDirectoryNotAllowed);
        }
        if cancellation.is_cancelled() {
            return Ok(ProcessResult {
                outcome: ProcessOutcome::Cancelled,
                exit_code: None,
                stdout: Vec::new(),
                stderr: Vec::new(),
                log_truncated: false,
                elapsed: Duration::ZERO,
            });
        }

        let mut command = Command::new(executable);
        command
            .args(arguments)
            .current_dir(&working_directory)
            .env_clear()
            .env("PATH", executable.parent().unwrap_or(Path::new("/")))
            .env("LANG", "C.UTF-8")
            .env("LC_ALL", "C.UTF-8")
            .env("TZ", "UTC")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_child(&mut command, &self.limits);
        let mut child = command.spawn().map_err(ProcessError::Io)?;
        let (sender, receiver) = mpsc::channel();
        let stdout = child.stdout.take().ok_or(ProcessError::MissingPipe)?;
        let stderr = child.stderr.take().ok_or(ProcessError::MissingPipe)?;
        let stdout_thread = reader_thread(stdout, OutputStream::Stdout, sender.clone());
        let stderr_thread = reader_thread(stderr, OutputStream::Stderr, sender);

        let started = Instant::now();
        let mut stdout_log = Vec::new();
        let mut stderr_log = Vec::new();
        let mut truncated = false;
        let (status, forced_outcome) = loop {
            drain_available(
                &receiver,
                &mut on_output,
                self.limits.max_log_bytes,
                &mut stdout_log,
                &mut stderr_log,
                &mut truncated,
            );
            if let Some(status) = child.try_wait().map_err(ProcessError::Io)? {
                break (status, None);
            }
            let outcome = if cancellation.is_cancelled() {
                Some(ProcessOutcome::Cancelled)
            } else if started.elapsed() >= self.limits.timeout {
                Some(ProcessOutcome::TimedOut)
            } else {
                None
            };
            if let Some(outcome) = outcome {
                let status = terminate_process_group(&mut child, self.limits.termination_grace)?;
                break (status, Some(outcome));
            }
            thread::sleep(Duration::from_millis(10));
        };

        stdout_thread
            .join()
            .map_err(|_| ProcessError::ReaderPanicked)??;
        stderr_thread
            .join()
            .map_err(|_| ProcessError::ReaderPanicked)??;
        drain_available(
            &receiver,
            &mut on_output,
            self.limits.max_log_bytes,
            &mut stdout_log,
            &mut stderr_log,
            &mut truncated,
        );

        let outcome = forced_outcome.unwrap_or_else(|| {
            if status.success() {
                ProcessOutcome::Succeeded
            } else {
                ProcessOutcome::Failed
            }
        });
        Ok(ProcessResult {
            outcome,
            exit_code: status.code(),
            stdout: stdout_log,
            stderr: stderr_log,
            log_truncated: truncated,
            elapsed: started.elapsed(),
        })
    }
}

fn reader_thread<R: Read + Send + 'static>(
    mut reader: R,
    stream: OutputStream,
    sender: mpsc::Sender<OutputChunk>,
) -> thread::JoinHandle<Result<(), ProcessError>> {
    thread::spawn(move || {
        let mut buffer = [0_u8; READ_CHUNK_BYTES];
        loop {
            let read = reader.read(&mut buffer).map_err(ProcessError::Io)?;
            if read == 0 {
                return Ok(());
            }
            if sender
                .send(OutputChunk {
                    stream,
                    bytes: buffer[..read].to_vec(),
                })
                .is_err()
            {
                return Ok(());
            }
        }
    })
}

fn drain_available<F>(
    receiver: &mpsc::Receiver<OutputChunk>,
    on_output: &mut F,
    max_bytes: usize,
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
    truncated: &mut bool,
) where
    F: FnMut(OutputChunk),
{
    while let Ok(chunk) = receiver.try_recv() {
        let remaining = max_bytes.saturating_sub(stdout.len() + stderr.len());
        let accepted = chunk.bytes.len().min(remaining);
        if accepted > 0 {
            let bounded = OutputChunk {
                stream: chunk.stream,
                bytes: chunk.bytes[..accepted].to_vec(),
            };
            on_output(bounded.clone());
            match bounded.stream {
                OutputStream::Stdout => stdout.extend_from_slice(&bounded.bytes),
                OutputStream::Stderr => stderr.extend_from_slice(&bounded.bytes),
            }
        }
        *truncated |= accepted < chunk.bytes.len();
    }
}

fn validate_name(name: &str) -> Result<(), ProcessError> {
    if !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(ProcessError::InvalidExecutable)
    }
}

#[cfg(unix)]
fn configure_child(command: &mut Command, limits: &ProcessLimits) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
    let address_space = limits.max_address_space_bytes;
    let file_size = limits.max_file_bytes;
    let open_files = limits.max_open_files;
    let cpu = limits.cpu_seconds;
    // SAFETY: pre_exec only invokes async-signal-safe libc setrlimit calls and
    // captures Copy integers. Failure aborts spawning the child.
    unsafe {
        command.pre_exec(move || {
            set_limit(libc::RLIMIT_AS, address_space)?;
            set_limit(libc::RLIMIT_FSIZE, file_size)?;
            set_limit(libc::RLIMIT_NOFILE, open_files)?;
            set_limit(libc::RLIMIT_CPU, cpu)?;
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure_child(_command: &mut Command, _limits: &ProcessLimits) {}

#[cfg(unix)]
fn set_limit(resource: libc::__rlimit_resource_t, value: u64) -> io::Result<()> {
    let limit = libc::rlimit {
        rlim_cur: value,
        rlim_max: value,
    };
    // SAFETY: limit points to a valid rlimit for the duration of the call.
    if unsafe { libc::setrlimit(resource, &limit) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn terminate_process_group(child: &mut Child, grace: Duration) -> Result<ExitStatus, ProcessError> {
    let group = -(child.id() as i32);
    // SAFETY: a negative pid addresses only the child-created process group.
    unsafe { libc::kill(group, libc::SIGTERM) };
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().map_err(ProcessError::Io)? {
            return Ok(status);
        }
        thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: same validated process group as above.
    unsafe { libc::kill(group, libc::SIGKILL) };
    child.wait().map_err(ProcessError::Io)
}

#[cfg(not(unix))]
fn terminate_process_group(
    child: &mut Child,
    _grace: Duration,
) -> Result<ExitStatus, ProcessError> {
    child.kill().map_err(ProcessError::Io)?;
    child.wait().map_err(ProcessError::Io)
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("process supervisor configuration is invalid")]
    InvalidConfiguration,
    #[error("executable is invalid")]
    InvalidExecutable,
    #[error("executable is not allowlisted")]
    ExecutableNotAllowed,
    #[error("working directory is outside approved roots")]
    WorkingDirectoryNotAllowed,
    #[error("child process pipe was not created")]
    MissingPipe,
    #[error("output reader thread panicked")]
    ReaderPanicked,
    #[error("process operation failed: {0}")]
    Io(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, thread};
    use tempfile::tempdir;

    fn supervisor(root: &Path, executable: &str, limits: ProcessLimits) -> ProcessSupervisor {
        ProcessSupervisor::new(
            BTreeMap::from([("test".to_owned(), PathBuf::from(executable))]),
            vec![root.to_path_buf()],
            limits,
        )
        .unwrap()
    }

    #[test]
    fn arguments_are_not_interpreted_by_a_shell() {
        let root = tempdir().unwrap();
        let marker = root.path().join("injected");
        let argument = OsString::from(format!("$(touch {})", marker.display()));
        let result = supervisor(root.path(), "/bin/echo", ProcessLimits::default())
            .run(
                "test",
                std::slice::from_ref(&argument),
                root.path(),
                &CancellationToken::default(),
                |_| {},
            )
            .unwrap();
        assert_eq!(result.outcome, ProcessOutcome::Succeeded);
        assert_eq!(result.stdout, [argument.as_encoded_bytes(), b"\n"].concat());
        assert!(!marker.exists());
    }

    #[test]
    fn bounds_captured_and_streamed_logs() {
        let root = tempdir().unwrap();
        let limits = ProcessLimits {
            max_log_bytes: 32,
            ..ProcessLimits::default()
        };
        let mut streamed = 0;
        let result = supervisor(root.path(), "/bin/sh", limits)
            .run(
                "test",
                &[OsString::from("-c"), OsString::from("yes x | head -c 4096")],
                root.path(),
                &CancellationToken::default(),
                |chunk| streamed += chunk.bytes.len(),
            )
            .unwrap();
        assert_eq!(result.stdout.len(), 32);
        assert!(result.log_truncated);
        assert_eq!(streamed, 32);
    }

    #[test]
    fn timeout_terminates_descendants() {
        let root = tempdir().unwrap();
        let pid_file = root.path().join("child.pid");
        let limits = ProcessLimits {
            timeout: Duration::from_millis(150),
            termination_grace: Duration::from_millis(50),
            ..ProcessLimits::default()
        };
        let script = format!("sleep 30 & echo $! > {}; wait", pid_file.display());
        let result = supervisor(root.path(), "/bin/sh", limits)
            .run(
                "test",
                &[OsString::from("-c"), OsString::from(script)],
                root.path(),
                &CancellationToken::default(),
                |_| {},
            )
            .unwrap();
        assert_eq!(result.outcome, ProcessOutcome::TimedOut);
        let pid: i32 = fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        thread::sleep(Duration::from_millis(30));
        // SAFETY: signal 0 performs only an existence check.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    }

    #[test]
    fn cancellation_is_race_safe() {
        let root = tempdir().unwrap();
        let token = CancellationToken::default();
        let trigger = token.clone();
        let thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            trigger.cancel();
        });
        let result = supervisor(root.path(), "/bin/sh", ProcessLimits::default())
            .run(
                "test",
                &[OsString::from("-c"), OsString::from("sleep 30")],
                root.path(),
                &token,
                |_| {},
            )
            .unwrap();
        thread.join().unwrap();
        assert_eq!(result.outcome, ProcessOutcome::Cancelled);
    }

    #[test]
    fn rejects_unknown_executable_and_outside_working_directory() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let supervisor = supervisor(root.path(), "/bin/echo", ProcessLimits::default());
        assert!(matches!(
            supervisor.run(
                "other",
                &[],
                root.path(),
                &CancellationToken::default(),
                |_| {}
            ),
            Err(ProcessError::ExecutableNotAllowed)
        ));
        assert!(matches!(
            supervisor.run(
                "test",
                &[],
                outside.path(),
                &CancellationToken::default(),
                |_| {}
            ),
            Err(ProcessError::WorkingDirectoryNotAllowed)
        ));
    }
}
