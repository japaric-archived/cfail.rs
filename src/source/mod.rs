//! Rust source file

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, self};
use std::ops::Deref;
use std::path::{AsPath, Path};

use {Annotations, LineMap, Span};

use self::parse::{Error, Parser};

pub mod parse;

/// The contents of a rust source file
pub struct Source(String);

impl Source {
    /// Opens a rust source file
    pub fn open<P: ?Sized>(path: &P) -> io::Result<Source> where
        P: AsPath,
    {
        Source::open_(path.as_path())
    }

    fn open_(source: &Path) -> io::Result<Source> {
        let mut contents = String::new();
        let mut file = try!(File::open(source));

        try!(file.read_to_string(&mut contents));

        Ok(Source(contents))
    }

    /// Parses the source file's annotations
    pub fn parse(&self) -> Result<LineMap<Annotations>, (Span, Error)> {
        use std::collections::btree_map::Entry::{Occupied, Vacant};

        let source: &str = &self;
        let mut map: LineMap<Annotations> = BTreeMap::new();

        let parser = Parser::new(source);

        for lka in parser {
            let (ln, kind, annotation) = try!(lka);

            match map.entry(ln) {
                Occupied(mut entry) => {
                    entry.get_mut().insert(kind, annotation)
                },
                Vacant(entry) => {
                    let mut annotations = Annotations::new();
                    annotations.insert(kind, annotation);
                    entry.insert(annotations);
                },
            }
        }

        Ok(map)
    }
}

impl Deref for Source {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}
