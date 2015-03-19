//! CLI tool

#![allow(deprecated)]

use std::{env, fmt};
use std::sync::mpsc;

use threadpool::ThreadPool;

use {Outcome, test};

/// Error: no arguments passed to `cfail`
struct NoArgs;

impl fmt::Display for NoArgs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("expected at least one argument, got none")
    }
}

fn run() -> Result<(), NoArgs> {

    let args: Vec<_> = env::args_os().skip(1).collect();
    let mut errors = 0;
    let mut failed = 0;
    let mut ignored = 0;
    let mut passed = 0;

    if args.is_empty() {
        return Err(NoArgs);
    }

    let ntests = args.len();
    let pool = ThreadPool::new(::std::os::num_cpus());
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
