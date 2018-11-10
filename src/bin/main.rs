extern crate clap;
#[macro_use] extern crate slog;
extern crate slog_async;
extern crate slog_term;

extern crate reprozip;

use std::os::unix::ffi::OsStrExt;
use std::process::exit;

use clap::{App, Arg, SubCommand};
use slog::Drain;

use reprozip::{Error, ExitStatus, Tracer};

struct LogLevelFilter<D> {
    drain: D,
    level: slog::Level,
}

impl<D> Drain for LogLevelFilter<D>
where
    D: Drain,
{
    type Ok = Option<D::Ok>;
    type Err = Option<D::Err>;

    fn log(
        &self,
        record: &slog::Record,
        values: &slog::OwnedKVList,
    ) -> Result<Self::Ok, Self::Err> {
        if record.level().is_at_least(self.level) {
            self.drain.log(record, values).map(Some).map_err(Some)
        } else {
            Ok(None)
        }
    }
}

fn main() {
    // Parse command line
    let mut cli = App::new("reprozip")
        .bin_name("reprozip")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(Arg::with_name("verbose")
             .short("v")
             .help("Augments verbosity")
             // broken, https://github.com/clap-rs/clap/issues/1356
             //.global(true)
             .multiple(true))
        .subcommand(SubCommand::with_name("trace")
                    .about("Execute a program and generate a trace")
                    .arg(Arg::with_name("rr4cmds")
                         .help("Command to run")
                         .required(true)
                         .takes_value(true)
                         .multiple(true)));
    let matches = match cli.get_matches_from_safe_borrow(std::env::args_os()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(2);
        }
    };

    // Set up logging to terminal
    let logger = {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let level = match matches.occurrences_of("verbose") {
            0 => slog::Level::Warning,
            1 => slog::Level::Info,
            2 => slog::Level::Debug,
            _ => slog::Level::Trace,
        };
        let drain = LogLevelFilter { drain, level }.fuse();
        let drain = slog_async::Async::new(drain)
            .overflow_strategy(slog_async::OverflowStrategy::Block)
            .build()
            .fuse();
        slog::Logger::root(drain, o!())
    };

    match matches.subcommand_name() {
        Some("trace") => {
            let s_matches = matches.subcommand_matches("trace").unwrap();
            let cmd = s_matches
                .values_of_os("rr4cmds")
                .expect("No value for 'command'")
                .into_iter()
                .map(OsStrExt::as_bytes)
                .collect();
            match run_trace(logger, cmd) {
                Ok(ExitStatus::Return(0)) => {}
                Ok(ExitStatus::Return(status)) => {
                    eprintln!(
                        "Warning: program returned non-zero exit status {}",
                        status
                    );
                }
                Ok(ExitStatus::Signal(sig)) => {
                    eprintln!("Warning: program was terminated by signal {:?}",
                              sig);
                }
                Err(err) => {
                    eprintln!("Error: {}", err);
                    exit(1);
                }
            }
        }
        _ => {
            cli.print_help().expect("Can't print help");
            std::process::exit(2);
        }
    }
}

fn run_trace(
    logger: slog::Logger,
    command: Vec<&[u8]>,
) -> Result<ExitStatus, Error> {
    Tracer::with_logger("/tmp/db", logger)?.trace(&command)
}
