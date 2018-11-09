//! This module is responsible for recording information in a SQLite database.

use std::borrow::Cow;
use std::path::Path;

use ::{Error, ExitStatus};

/// The ID assigned to a process in the database.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct ProcessId(u32);

bitflags! {
    /// Bit flags associated with a file access logged in the database.
    pub struct FileOp: u32 {
        /// File's previous content are read
        const READ  = 0b00001;
        /// New content is written to the file
        const WRITE = 0b00010;
        /// File is a directory, used as a process's working dir
        const WDIR  = 0b00100;
        /// File is stat()'d (only metadata is read)
        const STAT  = 0b01000;
        /// The link itself is accessed, no dereference
        const LINK  = 0b10000;
    }
}

/// The database, where we record events about the traced program.
pub struct Database {
    next_process: u32,
}

impl Database {
    pub fn new<D: AsRef<Path>>(path: D) -> Result<Database, Error> {
        Ok(Database { next_process: 0})
    }

    /// Record the creation of a thread or process.
    pub fn add_process(&mut self, parent: Option<ProcessId>,
                       working_dir: &Path, is_thread: bool)
        -> Result<ProcessId, Error>
    {
        // TODO
        let proc = self.next_process;
        self.next_process += 1;
        let parent_str = parent
            .map(|p| Cow::Owned(format!("{}", p.0)))
            .unwrap_or(Cow::Borrowed("(none)"));
        println!("Adding process {} parent={} is_thread={} working_dir={}",
                 proc, parent_str, is_thread, working_dir.to_string_lossy());
        Ok(ProcessId(proc))
    }

    /// Record a file access.
    pub fn add_file_open(&mut self, id: ProcessId,
                         path: &Path, mode: FileOp, is_directory: bool)
        -> Result<(), Error>
    {
        // TODO
        println!("Adding file open process={} path={} mode={:?}, \
                  is_directory={}",
                 id.0, path.to_string_lossy(), mode, is_directory);
        Ok(())
    }

    /// Record the death of a thread or process.
    pub fn process_exit(&mut self, id: ProcessId, status: ExitStatus)
        -> Result<(), Error>
    {
        // TODO
        println!("Adding process exit {} status={:?}",
                 id.0, status);
        Ok(())
    }

    /// Commit the trace to disk.
    pub fn commit(self) -> Result<(), Error> {
        // TODO
        Ok(())
    }
}
