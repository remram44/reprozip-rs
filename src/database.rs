use std::path::Path;

use ::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessId(u32);

bitflags! {
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

#[derive(Default)]
pub struct Database {
}

impl Database {
    pub fn add_first_process(&mut self, id: ProcessId, working_dir: &Path)
        -> Result<(), Error>
    {
        unimplemented!()
    }

    pub fn add_file_open(&mut self, id: ProcessId,
                         path: &Path, mode: FileOp, is_directory: bool)
        -> Result<(), Error>
    {
        unimplemented!()
    }

    pub fn commit(&mut self) -> Result<(), Error> {
        unimplemented!()
    }
}
