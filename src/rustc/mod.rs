//! The `rustc` compiler

use std::collections::BTreeMap;
use std::env;
use std::path::{AsPath, Path};
use std::process::Command;

use tempdir::TempDir;

use {Error, LineMap, Messages};

use self::parse::Parser;

pub mod parse;

/// Compiler stderr
pub struct Stderr {
    source: String,
    stderr: String,
}

/// Compiles a source file, and returns the compiler stderr
pub fn compile<P: ?Sized>(source: &P, library_path: &str) -> Result<Stderr, Error> where
    P: AsPath,
{
    Stderr::new(source.as_path(), library_path)
}

impl Stderr {
    fn new(path: &Path, library_path: &str) -> Result<Stderr, Error> {
        let current_dir = try!(env::current_dir());
        let temp_dir = try!(TempDir::new_in(&current_dir, "cfail"));
        let source = current_dir.join(path);

        let mut cmd = Command::new("rustc");
        cmd.current_dir(temp_dir.path());

        for path in library_path.split(':') {
            cmd.arg("-L").arg(&current_dir.join(path));
        }

        cmd.arg(&source);

        let output = try!(cmd.output());

        if output.status.success() {
            Err(Error::SuccessfulCompilation)
        } else {
            Ok(Stderr {
                source: source.to_string_lossy().into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }

    /// Parses the compiler stderr and returns a list of compiler messages
    pub fn parse(&self) -> Result<LineMap<Messages>, Error> {
        use std::collections::btree_map::Entry::{Occupied, Vacant};

        let mut map: LineMap<Messages> = BTreeMap::new();
        let stderr: &str = &self.stderr;

        let parser = Parser::new(stderr, &self.source);

        for lkm in parser {
            let (ln, kind, message) = try!(lkm);

            match map.entry(ln) {
                Occupied(mut entry) => {
                    entry.get_mut().insert(kind, message)
                },
                Vacant(entry) => {
                    let mut messages = Messages::new();
                    messages.insert(kind, message);
                    entry.insert(messages);
                },
            }
        }

        Ok(map)
    }
}
