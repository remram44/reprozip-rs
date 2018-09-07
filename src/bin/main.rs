extern crate reprozip;

use std::process::exit;

use reprozip::trace;

fn main() {
    match trace(&[b"echo", b"test"], "/tmp/db") {
        Ok(0) => {},
        Ok(status) => {
            eprintln!("Warning: program returned non-zero exit status {}",
                      status);
        }
        Err(err) => {
            eprintln!("Error: {}", err);
            exit(1);
        }
    }
}
