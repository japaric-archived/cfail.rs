//! CLI tool

use std::env::{VarError, self};
use std::fmt;
use std::sync::mpsc;

use num_cpus;
use threadpool::ThreadPool;

use {Outcome, test};

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::MalformedRustThreads => {
                f.write_str("the `RUST_THREADS` variable must contain a positive integer")
            },
            Error::NoArgs => {
                f.write_str("expected at least one argument, got none")
            },
        }
    }
}

enum Error {
    /// malformed `RUST_THREADS`
    MalformedRustThreads,
    /// no arguments passed to `cfail`
    NoArgs,
}

fn num_cpus() -> Result<usize, Error> {
    match env::var("RUST_THREADS") {
        Ok(threads) => match threads.parse() {
            Ok(threads) if threads > 0 => Ok(threads),
            _ => Err(Error::MalformedRustThreads),
        },
        Err(VarError::NotPresent) => Ok(num_cpus::get()),
        Err(_) => Err(Error::MalformedRustThreads),
    }
}

fn run() -> Result<(), Error> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let mut errors = 0;
    let mut failed = 0;
    let mut ignored = 0;
    let mut passed = 0;

    if args.is_empty() {
        return Err(Error::NoArgs);
    }

    let ntests = args.len();
    let pool = ThreadPool::new(try!(num_cpus()));
    let (tx, rx) = mpsc::channel();

    for path in args {
        let tx = tx.clone();
        pool.execute(move || {
            let outcome = test(&path);

            tx.send((path, outcome)).unwrap();
        });
    }

    for (path, outcome) in rx.iter().take(ntests) {
        let path = path.to_string_lossy();

        match outcome {
            Err(e) => {
                errors += 1;
                println!("{} ... ERROR\n{}", path, e);
            },
            Ok(Outcome::Failed(mismatches)) => {
                failed += 1;
                println!("{} ... FAILED\n{}", path, mismatches)
            },
            Ok(Outcome::Ignored) => {
                ignored += 1;
                println!("{} ... ignored", path);
            }
            Ok(Outcome::Passed) => {
                passed += 1;
                println!("{} ... ok", path);
            },
        }
    }

    println!("{} passed; {} failed; {} ignored; {} errored", passed, failed, ignored, errors);

    if failed > 0 || errors > 0 {
        env::set_exit_status(1);
    }

    Ok(())
}

/// The `main` function of the `cfail` binary
pub fn main() {
    if let Err(e) = run() {
        println!("error: {}", e);
        env::set_exit_status(1);
    }
}
