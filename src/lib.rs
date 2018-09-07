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

use nix::unistd::{ForkResult, Pid, fork, execvp};
use nix::sys::ptrace;
use nix::sys::signal::{Signal, kill};

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

impl StdError for Error {
}

/// Possible status of a thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThreadStatus {
    /// `fork()` done but not yet attached
    Allocated,
    /// Running process
    Attached,
    /// Attached but no corresponding fork() has returned yet
    Unknown,
}

/// A thread that we are tracking.
struct Thread {
    identifier: ProcessId,
    status: ThreadStatus,
    tid: Pid,
    thread_group: Rc<ThreadGroup>,
}

/// A group of threads, i.e. a process.
///
/// All the threads in a process share some attributes, such as the environment
/// and the working directory.
struct ThreadGroup {
    working_dir: PathBuf,
}

/// Structure holding all the running threads and processes.
#[derive(Default)]
struct Processes {
    pid2process: HashMap<Pid, Thread>,
    identifier2pid: HashMap<ProcessId, Pid>,
}

impl Processes {
    /// Add the first process, which has no parent.
    fn add_first(&mut self, tid: Pid, thread_group: Rc<ThreadGroup>,
                 status: ThreadStatus, database: &mut Database)
        -> Result<ProcessId, Error>
    {
        let identifier = database.add_process(
            None,
            &thread_group.working_dir,
            false,
        )?;
        self.pid2process.insert(
            tid,
            Thread {
                identifier,
                status,
                tid,
                thread_group,
            },
        );
        self.identifier2pid.insert(identifier, tid);
        Ok(identifier)
    }

    /// Add a new process, which was forked from another.
    fn add(&mut self, tid: Pid, thread_group: Rc<ThreadGroup>,
           status: ThreadStatus, parent: ProcessId, is_thread: bool,
           database: &mut Database)
        -> Result<ProcessId, Error>
    {
        let identifier = database.add_process(
            Some(parent),
            &thread_group.working_dir,
            is_thread,
        )?;
        self.pid2process.insert(
            tid,
            Thread {
                identifier,
                status,
                tid,
                thread_group,
            },
        );
        self.identifier2pid.insert(identifier, tid);
        Ok(identifier)
    }

    fn with_pid(&self, pid: Pid) -> &Thread {
        self.pid2process.get(&pid).unwrap()
    }

    fn with_pid_mut(&mut self, pid: Pid) -> &mut Thread {
        self.pid2process.get_mut(&pid).unwrap()
    }

    fn with_identifier(&self, id: ProcessId) -> &Thread {
        let pid = *self.identifier2pid.get(&id).unwrap();
        self.with_pid(pid)
    }

    fn with_identifier_mut(&mut self, id: ProcessId) -> &mut Thread {
        let pid = *self.identifier2pid.get(&id).unwrap();
        self.with_pid_mut(pid)
    }
}

/// Tracer following processes and logging their execution to a `Database`.
#[derive(Default)]
struct Tracer {
    processes: Processes,
    database: Database,
}

impl Tracer {
    fn trace<D: AsRef<Path>, C: AsRef<[u8]>>(
        self,
        command: &[C], database: D) -> Result<i32, Error>
    {
        self.trace_arg0(command, &command[0], database)
    }

    fn trace_arg0<D: AsRef<Path>, C: AsRef<[u8]>, C2: AsRef<[u8]>>(
        mut self,
        command: &[C], arg0: C2, database: D) -> Result<i32, Error>
    {
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
                    ThreadStatus::Allocated,
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
                        eprintln!("couldn't use ptrace: {}\n\
                                  This could be caused by a security policy or \
                                  isolation mechanism (such as Docker), see \
                                  http://bit.ly/2bZd8Fa",
                                  err);
                        std::process::exit(125);
                    }
                }
                // Stop this once so tracer can set options
                kill(Pid::this(), Signal::SIGSTOP).expect("Couldn't stop");
                // Execute the target
                match execvp(&arg0, &args) {
                    Ok(_) => unreachable!(),
                    Err(err) => {
                        eprintln!("Coundn't execute the target command: {}", err);
                        std::process::exit(127);
                    }
                }
            }
            Err(err) => return Err(Error::Internal(format!("fork: {}", err))),
        }
    }

    fn trace_process(&mut self, child: Pid) -> Result<i32, Error> {
        unimplemented!()
    }
}

/// Run a command and trace it.
pub fn trace<D: AsRef<Path>, C: AsRef<[u8]>>(
    command: &[C], database: D) -> Result<i32, Error>
{
    <Tracer as Default>::default().trace(command, database)
}

/// Run a command and trace it, replacing `argv[0]`.
///
/// For example:
/// ```rust,no_run
/// # use reprozip::trace_arg0;
/// trace_arg0(&[b"/bin/busybox", b"hello world!"], b"echo", "/tmp/db");
/// ```
pub fn trace_arg0<D: AsRef<Path>, C: AsRef<[u8]>, C2: AsRef<[u8]>>(
    command: &[C], arg0: C2, database: D) -> Result<i32, Error>
{
    <Tracer as Default>::default().trace_arg0(command, arg0, database)
}
