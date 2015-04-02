//! Compile fail testing

#![deny(missing_docs)]
#![deny(warnings)]
#![feature(collections)]
#![feature(exit_status)]
#![feature(into_cow)]
#![feature(slice_patterns)]
#![feature(unicode)]

extern crate num_cpus;
extern crate tempdir;
extern crate threadpool;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::{Add, Sub};
use std::path::Path;
use std::{env, fmt, io};

pub mod driver;
pub mod match_;
pub mod rustc;
pub mod source;

/// Source file line number
#[derive(Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Line(u32);

impl Add<u32> for Line {
    type Output = Line;

    fn add(self, rhs: u32) -> Line {
        Line(self.0 + rhs)
    }
}

impl Sub<u32> for Line {
    type Output = Option<Line>;

    fn sub(self, rhs: u32) -> Option<Line> {
        self.0.checked_sub(rhs).and_then(|line| {
            if line == 0 {
                None
            } else {
                Some(Line(line))
            }
        })
    }
}

/// Source file span
#[derive(Copy, Debug)]
pub struct Span(BytePos, BytePos);

impl Add<BytePos> for Span {
    type Output = Span;

    fn add(self, offset: BytePos) -> Span {
        Span(self.0 + offset, self.1 + offset)
    }
}

impl Sub<BytePos> for Span {
    type Output = Option<Span>;

    fn sub(self, offset: BytePos) -> Option<Span> {
        match (self.0.checked_sub(offset), self.1.checked_sub(offset)) {
            (Some(start), Some(end)) => Some(Span(start, end)),
            _ => None,
        }
    }
}

/// Byte position
pub type BytePos = usize;

/// Map: `Line` -> `Annotations`/`Messages`
pub type LineMap<T> = BTreeMap<Line, T>;

/// Errors
#[derive(Debug)]
pub enum Error {
    /// IO error
    Io(io::Error),
    /// Error parsing the source file
    ParseSource(String),
    /// Error parsing the compiler stderr
    ParseStderr(String),
    /// Source file successfully compiled
    SuccessfulCompilation,
    /// Unsupported feature
    Unsupported(Feature),
}

/// Unsupported `cfail` features
#[derive(Debug)]
pub enum Feature {
    /// Auxiliar build
    AuxBuild,
    /// Error pattern
    ErrorPattern,
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Feature::AuxBuild => f.write_str("auxiliar builds"),
            Feature::ErrorPattern => f.write_str("error patterns"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::Io(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Io(ref e) => {
                write!(f, "{}", e)
            },
            Error::ParseSource(ref err) => {
                f.write_str(err)
            },
            Error::ParseStderr(ref line) => {
                write!(f, "couldn't parse stderr: {}", line)
            },
            Error::SuccessfulCompilation => {
                write!(f, "compilation succeeded")
            },
            Error::Unsupported(ref feature) => {
                write!(f, "{} are not currently supported", feature)
            }
        }
    }
}

const NKINDS: usize = 4;
const KINDS: [Kind; 4] = [Kind::Error, Kind::Warning, Kind::Help, Kind::Note];

/// "Kind" of compiler messages
#[derive(Copy, Debug, PartialEq)]
pub enum Kind {
    /// `error`
    Error,
    /// `help`
    Help,
    /// `note`
    Note,
    /// `warning`
    Warning,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Kind::Error => f.write_str("error"),
            Kind::Help => f.write_str("help"),
            Kind::Note => f.write_str("note"),
            Kind::Warning => f.write_str("warning"),
        }
    }
}

impl Kind {
    fn needle(&self) -> &'static str {
        match *self {
             Kind::Error => "error",
             Kind::Help => "help",
             Kind::Note => "note",
             Kind::Warning => "warning",
        }
    }
}

/// `cfail` annotations
#[derive(Debug)]
pub struct Annotations<'a>([Option<Vec<Cow<'a, str>>>; NKINDS]);

impl<'a> Annotations<'a> {
    fn new() -> Annotations<'a> {
        Annotations([None, None, None, None])
    }

    fn insert(&mut self, kind: Kind, annotation: Cow<'a, str>) {
        if let Some(ref mut anns) = self.0[kind as usize] {
            anns.push(annotation)
        } else {
            self.0[kind as usize] = Some(vec![annotation])
        }
    }

    fn take(&mut self, kind: Kind) -> Option<Vec<Cow<'a, str>>> {
        self.0[kind as usize].take()
    }
}

/// Compiler messages
#[derive(Debug)]
pub struct Messages<'a>([Option<Vec<&'a str>>; NKINDS]);

impl<'a> Messages<'a> {
    fn new() -> Messages<'a> {
        Messages([None, None, None, None])
    }

    fn insert(&mut self, kind: Kind, message: &'a str) {
        if let Some(ref mut msgs) = self.0[kind as usize] {
            msgs.push(message)
        } else {
            self.0[kind as usize] = Some(vec![message])
        }
    }

    fn take(&mut self, kind: Kind) -> Option<Vec<&'a str>> {
        self.0[kind as usize].take()
    }
}

/// The outcome of the `cfail` test
pub enum Outcome {
    /// The test failed
    Failed(String),
    /// The test was ignored
    Ignored,
    /// The test passed
    Passed,
}

/// Performs a compile fail test on a source file
///
/// This function
///
/// Note: this function should never panic, if it does that's a bug
pub fn test<P: ?Sized>(source: &P) -> Result<Outcome, Error> where P: AsRef<Path> {
    fn test_(path: &Path) -> Result<Outcome, Error> {
        use source::Source;
        use rustc;

        let source = try!(Source::open(&path));
        if source.contains("// ignore-test") {
            return Ok(Outcome::Ignored)
        }

        if source.contains("// aux-build") {
            return Err(Error::Unsupported(Feature::AuxBuild))
        }

        if source.contains("// error-pattern") {
            return Err(Error::Unsupported(Feature::ErrorPattern))
        }

        let annotations = match source.parse() {
            Err((span, e)) => {
                return Err(Error::ParseSource(source::parse::format_error(path, &source, span, e)))
            },
            Ok(annotations) => annotations,
        };

        let library_path = env::var("CFAIL_LIBRARY_PATH").unwrap_or(String::new());
        let output = try!(rustc::compile(&path, &library_path));
        let messages = try!(output.parse());

        let mismatches = match_::match_(annotations, messages);

        if mismatches.get(Kind::Error).is_none() && mismatches.get(Kind::Warning).is_none() {
            Ok(Outcome::Passed)
        } else {
            Ok(Outcome::Failed(match_::format(mismatches)))
        }
    }

    test_(source.as_ref())
}
