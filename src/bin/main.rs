extern crate reprozip;

use std::process::exit;

use reprozip::{ExitStatus, trace};

fn main() {
    match trace(&[b"echo", b"test"], "/tmp/db") {
        Ok(ExitStatus::Return(0)) => {},
        Ok(ExitStatus::Return(status)) => {
            eprintln!("Warning: program returned non-zero exit status {}",
                      status);
        }
        Ok(ExitStatus::Signal(sig)) => {
            eprintln!("Warning: program was terminated by signal {:?}", sig);
        }
        Err(err) => {
            eprintln!("Error: {}", err);
            exit(1);
        }
    }
}
