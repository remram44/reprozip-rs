#[macro_use] extern crate bitflags;
extern crate nix;

mod database;

use std::collections::HashMap;
use std::env::current_dir;
use std::error::Error as StdError;
use std::ffi::CString;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use nix::Error as NixError;
use nix::sys::ptrace;
use nix::sys::signal::{Signal, kill};
use nix::sys::wait;
use nix::unistd::{ForkResult, Pid, fork, execvp};

use database::{Database, FileOp, ProcessId};

/// General error type returned by this crate.
#[derive(Debug)]
pub enum Error {
    InvalidCommand,
    Internal(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            &Error::InvalidCommand => write!(f, "Invalid command"),
            &Error::Internal(ref s) => write!(f, "{}", s),
        }
    }
}

impl StdError for Error {}

impl From<NixError> for Error {
    fn from(err: NixError) -> Error {
        Error::Internal(format!("{}", err))
    }
}

/// Exit status from a process, either a return code or a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    Return(i32),
    Signal(Signal),
}

/// A group of threads, i.e. a process.
///
/// All the threads in a process share some attributes, such as the environment
/// and the working directory.
struct ThreadGroup {
    working_dir: PathBuf,
}

/// A thread that we are tracking.
enum Thread {
    Unknown { tid: Pid },
    Allocated(ThreadInfo),
    Attached(ThreadInfo),
}

#[derive(Clone)]
struct ThreadInfo {
    identifier: ProcessId,
    tid: Pid,
    thread_group: Rc<ThreadGroup>,
}

impl ThreadInfo {
    fn exit(
        self,
        exitstatus: ExitStatus,
        database: &mut Database,
    ) -> Result<(), Error> {
        database.process_exit(self.identifier, exitstatus)
    }
}

/// Structure holding all the running threads and processes.
#[derive(Default)]
struct Processes {
    pid2process: HashMap<Pid, Thread>,
    identifier2pid: HashMap<ProcessId, Pid>,
}

impl Processes {
    /// Add the first process, which has no parent.
    fn add_first(
        &mut self,
        tid: Pid,
        thread_group: Rc<ThreadGroup>,
        database: &mut Database,
    ) -> Result<ProcessId, Error> {
        let identifier =
            database.add_process(None, &thread_group.working_dir, false)?;
        self.pid2process.insert(
            tid,
            Thread::Allocated(ThreadInfo {
                identifier,
                tid,
                thread_group,
            }),
        );
        self.identifier2pid.insert(identifier, tid);
        Ok(identifier)
    }

    /// Add a new process, currently unknown.
    ///
    /// This is required because we can see processes appear before we see
    /// their creator returning from fork().
    fn add_unknown(&mut self, tid: Pid) -> Result<(), Error> {
        self.pid2process.insert(tid, Thread::Unknown { tid });
        Ok(())
    }

    fn exit(
        &mut self,
        tid: Pid,
        exitstatus: ExitStatus,
        database: &mut Database,
    ) -> Result<(), Error> {
        let thread = self.pid2process.remove(&tid).unwrap();
        match thread {
            Thread::Allocated(info) | Thread::Attached(info) => {
                self.identifier2pid.remove(&info.identifier);
                info.exit(exitstatus, database)?;
            }
            Thread::Unknown { .. } => {}
        }
        println!("Process exited, {} processes remain",
                 self.pid2process.len());
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.pid2process.is_empty()
    }

    fn has_pid(&self, pid: Pid) -> bool {
        self.pid2process.contains_key(&pid)
    }

    fn get_pid(&self, pid: Pid) -> &Thread {
        self.pid2process.get(&pid).unwrap()
    }

    fn get_pid_mut(&mut self, pid: Pid) -> &mut Thread {
        self.pid2process.get_mut(&pid).unwrap()
    }

    fn get_identifier(&self, id: ProcessId) -> &Thread {
        let pid = *self.identifier2pid.get(&id).unwrap();
        self.get_pid(pid)
    }

    fn get_identifier_mut(&mut self, id: ProcessId) -> &mut Thread {
        let pid = *self.identifier2pid.get(&id).unwrap();
        self.get_pid_mut(pid)
    }
}

/// Tracer following processes and logging their execution to a `Database`.
struct Tracer {
    processes: Processes,
    database: Database,
}

impl Tracer {
    fn new<D: AsRef<Path>>(database: D) -> Result<Tracer, Error> {
        Ok(Tracer {
            processes: Default::default(),
            database: Database::new(database)?,
        })
    }

    fn trace<C: AsRef<[u8]>>(
        self,
        command: &[C],
    ) -> Result<ExitStatus, Error> {
        self.trace_arg0(command, &command[0])
    }

    fn trace_arg0<C: AsRef<[u8]>, C2: AsRef<[u8]>>(
        mut self,
        command: &[C],
        arg0: C2,
    ) -> Result<ExitStatus, Error> {
        let args = {
            let mut vec = Vec::new();
            for c in command.into_iter() {
                match CString::new(c.as_ref()) {
                    Ok(c) => vec.push(c),
                    Err(_) => return Err(Error::InvalidCommand),
                }
            }
            vec
        };
        let arg0 = match CString::new(arg0.as_ref()) {
            Ok(c) => c,
            Err(_) => return Err(Error::InvalidCommand),
        };
        println!("Tracing command: {:?}", args);

        match fork() {
            Ok(ForkResult::Parent { child }) => {
                println!("Child created, pid={}", child);
                let wd = current_dir().unwrap();
                let identifier = self.processes.add_first(
                    child,
                    Rc::new(ThreadGroup {
                        working_dir: wd.clone(),
                    }),
                    &mut self.database,
                )?;
                self.database.add_file_open(identifier, &wd,
                                            FileOp::WDIR, true)?;
                let ret = self.trace_process(child)?;
                self.database.commit()?;
                Ok(ret)
            }
            Ok(ForkResult::Child) => {
                // Trace this process
                match ptrace::traceme() {
                    Ok(()) => {}
                    Err(err) => {
                        eprintln!(
                            "couldn't use ptrace: {}\nThis could be caused \
                             by a security policy or isolation mechanism \
                             (such as Docker), see http://bit.ly/2bZd8Fa",
                            err
                        );
                        std::process::exit(125);
                    }
                }
                // Stop this once so tracer can set options
                kill(Pid::this(), Signal::SIGSTOP).expect("Couldn't stop");
                // Execute the target
                match execvp(&arg0, &args) {
                    Ok(_) => unreachable!(),
                    Err(err) => {
                        eprintln!("Coundn't execute the target command: {}",
                                  err);
                        std::process::exit(127);
                    }
                }
            }
            Err(err) => return Err(Error::Internal(format!("fork: {}", err))),
        }
    }

    /// Main tracing procedure, waits for events.
    fn trace_process(&mut self, first_proc: Pid) -> Result<ExitStatus, Error> {
        let mut first_exit_code = None;
        loop {
            match wait::waitpid(Pid::from_raw(-1),
                                Some(wait::WaitPidFlag::__WALL))? {
                // A program exited
                wait::WaitStatus::Exited(pid, status) => {
                    let exitstatus = ExitStatus::Return(status);
                    if pid == first_proc {
                        first_exit_code = Some(exitstatus);
                    }
                    self.processes.exit(pid, exitstatus, &mut self.database)?;
                    if self.processes.is_empty() {
                        break;
                    }
                    continue;
                }
                wait::WaitStatus::Signaled(pid, sig, _) => {
                    let exitstatus = ExitStatus::Signal(sig);
                    if pid == first_proc {
                        first_exit_code = Some(exitstatus);
                    }
                    self.processes.exit(pid, exitstatus, &mut self.database)?;
                    if self.processes.is_empty() {
                        break;
                    }
                    continue;
                }
                wait::WaitStatus::PtraceEvent(pid, sig, event) => {
                    println!("ptrace event");
                    // TODO: handle events, tracer.c:521
                    ptrace::syscall(pid)?;
                }
                wait::WaitStatus::Stopped(pid, sig) => {
                    if !self.processes.has_pid(pid) {
                        println!("process {} appeared", pid);
                        self.processes.add_unknown(pid)?;
                        Self::set_options(pid)?;
                        // Don't resume, it will be set to ATTACHED and resumed
                        // when the parent returns from fork()
                        continue;
                    }
                    let thread = self.processes.get_pid_mut(pid);
                    if let Some(info) = if let Thread::Allocated(info) = thread
                    {
                        // Have to do this in two steps to avoid borrow error
                        Some(info.clone())
                    } else {
                        None
                    } {
                        println!("process {} attached", pid);
                        *thread = Thread::Attached(info);
                        Self::set_options(pid)?;
                        ptrace::syscall(pid)?;
                        continue;
                    }

                    println!("stopped");
                    if sig == Signal::SIGTRAP {
                        eprintln!("NOT delivering SIGTRAP to {}", pid);
                        ptrace::syscall(pid)?;
                    } else {
                        println!("Caught signal {:?}", sig);
                        if ptrace::getsiginfo(pid).is_ok() {
                            ptrace::syscall(pid)?;
                        } else {
                            eprintln!("NOT delivering {:?} to {}", sig, pid);
                            if sig != Signal::SIGSTOP {
                                ptrace::syscall(pid)?;
                            }
                        }
                    }
                }
                wait::WaitStatus::PtraceSyscall(pid) => {
                    println!("ptrace syscall");
                    // TODO: syscall, tracer.c:423
                    ptrace::syscall(pid)?;
                }
                _ => {}
            }
        }
        Ok(first_exit_code.expect("Trace finished but we never got the first \
                                   process' exit code"))
    }

    fn set_options(pid: Pid) -> Result<(), Error> {
        ptrace::setoptions(
            pid,
            ptrace::Options::PTRACE_O_TRACESYSGOOD
                | ptrace::Options::PTRACE_O_EXITKILL
                | ptrace::Options::PTRACE_O_TRACECLONE
                | ptrace::Options::PTRACE_O_TRACEFORK
                | ptrace::Options::PTRACE_O_TRACEVFORK
                | ptrace::Options::PTRACE_O_TRACEEXEC,
        )?;
        Ok(())
    }
}

/// Run a command and trace it.
pub fn trace<D: AsRef<Path>, C: AsRef<[u8]>>(
    command: &[C],
    database: D,
) -> Result<ExitStatus, Error> {
    Tracer::new(database)?.trace(command)
}

/// Run a command and trace it, replacing `argv[0]`.
///
/// For example:
/// ```rust,no_run
/// # use reprozip::trace_arg0;
/// trace_arg0(&[b"/bin/busybox", b"hello world!"], b"echo", "/tmp/db");
/// ```
pub fn trace_arg0<D: AsRef<Path>, C: AsRef<[u8]>, C2: AsRef<[u8]>>(
    command: &[C],
    arg0: C2,
    database: D,
) -> Result<ExitStatus, Error> {
    Tracer::new(database)?.trace_arg0(command, arg0)
}
