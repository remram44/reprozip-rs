#[macro_use] extern crate bitflags;
extern crate nix;

mod database;

use std::collections::HashMap;
use std::env::current_dir;
use std::error::Error as StdError;
use std::ffi::CString;
use std::fmt::Display;
use std::path::{Path, PathBuf};

use nix::sys::ptrace;
use nix::sys::wait::waitpid;
use nix::unistd::{ForkResult, Pid, fork, execvp};
use nix::sys::signal::{Signal, kill};

use database::{Database, FileOp, ProcessId};

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

struct Process {
    identifier: ProcessId,
}

struct ThreadGroup {
    working_dir: PathBuf,
}

#[derive(Default)]
struct Processes {
    processes: Vec<Process>,
}

impl Processes {
    fn new(&mut self, pid: Pid) -> &mut Process {
        unimplemented!()
    }
}

#[derive(Default)]
struct Tracer {
    processes: Processes,
    threadgroups: HashMap<i32, ThreadGroup>,
    database: Database,
}

impl Tracer {
    pub fn new() -> Tracer {
        Default::default()
    }

    pub fn trace<D: AsRef<Path>, C: AsRef<[u8]>>(
        &mut self,
        command: &[C], database: D) -> Result<i32, Error>
    {
        self.trace_arg0(command, &command[0], database)
    }

    pub fn trace_arg0<D: AsRef<Path>, C: AsRef<[u8]>, C2: AsRef<[u8]>>(
        &mut self,
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
                self.threadgroups.insert(child.into(), ThreadGroup {
                    working_dir: wd.clone(),
                });
                {
                    let process = self.processes.new(child);
                    self.database.add_first_process(process.identifier, &wd)?;
                    self.database.add_file_open(process.identifier, &wd,
                                                FileOp::WDIR, true)?;
                }
                let ret = self.trace_process(child);
                self.database.commit();
                ret
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

pub fn trace<D: AsRef<Path>, C: AsRef<[u8]>>(
    command: &[C], database: D) -> Result<i32, Error>
{
    Tracer::new().trace(command, database)
}
