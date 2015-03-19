//! The `rustc` compiler
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::env;
use std::path::{AsPath, Path};
use std::process::Command;

use tempdir::TempDir;

use {Error, LineMap, Messages};

use self::parse::Parser;

pub mod parse;

/// Compiler stderr
pub struct Stderr<'a> {
    source: Cow<'a, str>,
    stderr: String,
}

/// Compiles a source file, and returns the compiler stderr
pub fn compile<P: ?Sized>(source: &P) -> Result<Stderr, Error> where
    P: AsPath,
{
    Stderr::new(source.as_path())
}

impl<'a> Stderr<'a> {
    fn new(path: &Path) -> Result<Stderr, Error> {
        let temp_dir = try!(TempDir::new("cfail"));
        let source = {
            let mut cwd = try!(env::current_dir());
            cwd.push(path);
            cwd
        };

        let output = try!(Command::new("rustc").
            current_dir(temp_dir.path()).
            arg(&source).
            output());

        if output.status.success() {
            Err(Error::SuccessfulCompilation)
        } else {
            Ok(Stderr {
                source: path.to_string_lossy(),
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
