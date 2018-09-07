//! This module is responsible for recording information in a SQLite database.

use std::path::Path;

use ::Error;

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
#[derive(Default)]
pub struct Database {
}

impl Database {
    /// Record the creation of a thread or process.
    pub fn add_process(&mut self, parent: Option<ProcessId>,
                       working_dir: &Path, is_thread: bool)
        -> Result<ProcessId, Error>
    {
        unimplemented!()
    }

    /// Record a file access.
    pub fn add_file_open(&mut self, id: ProcessId,
                         path: &Path, mode: FileOp, is_directory: bool)
        -> Result<(), Error>
    {
        unimplemented!()
    }

    /// Commit the trace to disk.
    pub fn commit(self) -> Result<(), Error> {
        unimplemented!()
    }
}
