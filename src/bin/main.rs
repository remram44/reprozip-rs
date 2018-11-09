#[macro_use] extern crate slog;
extern crate slog_async;
extern crate slog_term;

extern crate reprozip;

use std::process::exit;

use slog::Drain;

use reprozip::{Error, ExitStatus, Tracer};

fn main() {
    let logger = {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain)
            .overflow_strategy(slog_async::OverflowStrategy::Block)
            .build()
            .fuse();
        slog::Logger::root(drain, o!())
    };

    match run(logger) {
        Ok(ExitStatus::Return(0)) => {}
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

fn run(logger: slog::Logger) -> Result<ExitStatus, Error> {
    Tracer::with_logger("/tmp/db", logger)?.trace(&[b"echo", b"test"])
}
