//! Ported from burningtnt/Terracotta@694980b src/easytier/executable_impl.rs (process lifecycle only).
//! Local changes: binaries come from a detected install, output goes to crate::logs, the child follows parent death, Android and console parts dropped.

use std::env;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::thread;
use std::time::Duration;

use crate::easytier::argument::{Argument, command_arguments};
use crate::easytier::ports::PortRequest;
use crate::logs::Logs;

const AREA: &str = "EasyTier";
const EXIT_LINES: usize = 500;
const POLL: Duration = Duration::from_millis(100);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct LaunchConfig {
    pub core: PathBuf,
    pub cli: PathBuf,
    pub arguments: Vec<Argument>,
}

#[derive(Debug)]
pub enum ProcessError {
    MissingExecutable { path: PathBuf },
    Spawn { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingExecutable { path } => {
                write!(
                    formatter,
                    "{}: EasyTier executable not found",
                    path.display()
                )
            }
            Self::Spawn { path, source } => {
                write!(
                    formatter,
                    "{}: failed to start EasyTier: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingExecutable { .. } => None,
            Self::Spawn { source, .. } => Some(source),
        }
    }
}

pub struct EasyTierProcess {
    child: Arc<Mutex<Child>>,
    cli: PathBuf,
    logs: Arc<Logs>,
    rpc: u16,
    #[cfg(windows)]
    _job: Option<job::KillOnClose>,
}

pub fn spawn(config: LaunchConfig, logs: Arc<Logs>) -> Result<EasyTierProcess, ProcessError> {
    let LaunchConfig {
        core,
        cli,
        arguments,
    } = config;
    if !core.is_file() {
        return Err(ProcessError::MissingExecutable { path: core });
    }
    let rpc = PortRequest::EasyTierRPC.request();
    let room_credentials: Vec<_> = arguments
        .iter()
        .filter_map(|argument| match argument {
            Argument::NetworkName(value) | Argument::NetworkSecret(value) if !value.is_empty() => {
                Some(value.to_string())
            }
            _ => None,
        })
        .collect();
    let arguments = command_arguments(arguments);
    logs.activity(AREA, &format!("starting {} rpc={rpc}", core.display()));

    let mut command = Command::new(&core);
    command
        .args(&arguments)
        .args(["-r", &rpc.to_string()])
        .current_dir(env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(target_os = "linux")]
    parent_death::prepare(&mut command);

    let mut child = command
        .spawn()
        .map_err(|source| ProcessError::Spawn { path: core, source })?;

    #[cfg(windows)]
    let job = match job::KillOnClose::attach(&child) {
        Ok(job) => Some(job),
        Err(error) => {
            logs.activity(AREA, &format!("cannot create the job object: {error}"));
            None
        }
    };

    let (sender, receiver) = mpsc::channel();
    forward_std(&mut child, sender);

    let child = Arc::new(Mutex::new(child));
    let watched = Arc::clone(&child);
    let monitor_logs = Arc::clone(&logs);
    thread::spawn(move || monitor(watched, receiver, monitor_logs, room_credentials));

    Ok(EasyTierProcess {
        child,
        cli,
        logs,
        rpc,
        #[cfg(windows)]
        _job: job,
    })
}

impl EasyTierProcess {
    pub fn rpc_port(&self) -> u16 {
        self.rpc
    }

    pub fn is_alive(&self) -> bool {
        matches!(lock(self.child.as_ref()).try_wait(), Ok(None))
    }

    pub fn wait(&self) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = lock(self.child.as_ref()).try_wait()? {
                return Ok(status);
            }
            thread::sleep(POLL);
        }
    }

    pub fn kill(&self) {
        let mut child = lock(self.child.as_ref());
        let _ = child.kill();
        let _ = child.wait();
    }

    pub fn peers_json(&self) -> io::Result<String> {
        let output = self.cli_command().output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(io::Error::other(format!(
                "{}: easytier-cli exited with {:?}: {}",
                self.cli.display(),
                output.status.code(),
                stderr.trim()
            )));
        }
        String::from_utf8(output.stdout).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {error}", self.cli.display()),
            )
        })
    }

    fn cli_command(&self) -> Command {
        let mut command = Command::new(&self.cli);
        let rpc = format!("127.0.0.1:{}", self.rpc);
        command
            .args(["-p", &rpc, "-o", "json", "peer", "list"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }
}

impl Drop for EasyTierProcess {
    fn drop(&mut self) {
        if self.is_alive() {
            self.logs.activity(AREA, "killing EasyTier");
        }
        self.kill();
    }
}

fn forward_std(child: &mut Child, sender: mpsc::Sender<String>) {
    if let Some(stdout) = child.stdout.take() {
        let sender = sender.clone();
        thread::spawn(move || read_lines(BufReader::new(stdout), sender));
    }
    if let Some(stderr) = child.stderr.take() {
        thread::spawn(move || read_lines(BufReader::new(stderr), sender));
    }
}

fn read_lines<R: BufRead>(reader: R, sender: mpsc::Sender<String>) {
    for line in reader.lines() {
        let Ok(line) = line else { return };
        if sender.send(line).is_err() {
            return;
        }
    }
}

fn monitor(
    child: Arc<Mutex<Child>>,
    receiver: mpsc::Receiver<String>,
    logs: Arc<Logs>,
    room_credentials: Vec<String>,
) {
    let mut buffer: [Option<String>; EXIT_LINES] = std::array::from_fn(|_| None);
    let mut index = 0;
    let mut closed = false;
    let mut status: Option<ExitStatus> = None;
    loop {
        if !closed {
            match receiver.recv_timeout(POLL) {
                Ok(line) => {
                    buffer[index] = Some(line);
                    index = (index + 1) % EXIT_LINES;
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => closed = true,
            }
        }
        match lock(child.as_ref()).try_wait() {
            Ok(Some(exit)) => {
                status = Some(exit);
                break;
            }
            Ok(None) => {}
            Err(error) => {
                logs.activity(
                    AREA,
                    &format!("cannot read the EasyTier exit status: {error}"),
                );
                break;
            }
        }
        thread::sleep(POLL);
    }
    let mut summary = String::from("EasyTier exited");
    match status {
        Some(status) => summary.push_str(&format!(
            " with code {:?} (success={})",
            status.code(),
            status.success()
        )),
        None => summary.push_str(" with an unknown status"),
    }
    summary.push_str(". Output:\n############################################################");
    for offset in 0..EXIT_LINES {
        if let Some(line) = &buffer[(index + offset) % EXIT_LINES] {
            summary.push_str("\n    ");
            summary.push_str(line);
        }
    }
    summary.push_str("\n############################################################");
    for value in room_credentials {
        summary = summary.replace(&value, "[redacted]");
    }
    logs.activity(AREA, &summary);
}

fn lock(child: &Mutex<Child>) -> MutexGuard<'_, Child> {
    child
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(windows)]
mod job {
    use std::ffi::c_void;
    use std::io;
    use std::mem::size_of;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::process::Child;
    use std::ptr;

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;

    #[repr(C)]
    struct BasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    struct ExtendedLimitInformation {
        basic_limit_information: BasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            job: *mut c_void,
            class: i32,
            information: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    }

    pub struct KillOnClose {
        _handle: OwnedHandle,
    }

    impl KillOnClose {
        pub fn attach(child: &Child) -> io::Result<Self> {
            // SAFETY: the structs mirror the documented Win32 layouts, and every handle is owned or borrowed for the duration of the call.
            unsafe {
                let raw = CreateJobObjectW(ptr::null(), ptr::null());
                if raw.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let job = OwnedHandle::from_raw_handle(raw);
                let information = ExtendedLimitInformation {
                    basic_limit_information: BasicLimitInformation {
                        per_process_user_time_limit: 0,
                        per_job_user_time_limit: 0,
                        limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                        minimum_working_set_size: 0,
                        maximum_working_set_size: 0,
                        active_process_limit: 0,
                        affinity: 0,
                        priority_class: 0,
                        scheduling_class: 0,
                    },
                    io_info: IoCounters {
                        read_operation_count: 0,
                        write_operation_count: 0,
                        other_operation_count: 0,
                        read_transfer_count: 0,
                        write_transfer_count: 0,
                        other_transfer_count: 0,
                    },
                    process_memory_limit: 0,
                    job_memory_limit: 0,
                    peak_process_memory_used: 0,
                    peak_job_memory_used: 0,
                };
                let configured = SetInformationJobObject(
                    job.as_raw_handle(),
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    (&information as *const ExtendedLimitInformation).cast(),
                    size_of::<ExtendedLimitInformation>() as u32,
                );
                if configured == 0 {
                    return Err(io::Error::last_os_error());
                }
                if AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Self { _handle: job })
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod parent_death {
    use std::io;
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    const PR_SET_PDEATHSIG: i32 = 1;
    const SIGKILL: i32 = 9;

    unsafe extern "C" {
        fn prctl(option: i32, ...) -> i32;
    }

    pub fn prepare(command: &mut Command) {
        // SAFETY: the hook only calls async-signal-safe prctl in the forked child before exec.
        unsafe {
            command.pre_exec(|| {
                if prctl(PR_SET_PDEATHSIG, SIGKILL) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
}
