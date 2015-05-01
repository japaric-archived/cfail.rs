//! `cfail` annotation parser

use std::borrow::{Cow, IntoCow};
use std::fmt;
use std::iter::{Peekable, self};
use std::path::Path;
use std::str::Lines;

use unicode_width::UnicodeWidthStr;

use {BytePos, Kind, Line, Span};

use self::lexer::{Lexer, Token};

pub mod lexer;

/// Parse errors
#[derive(Clone, Copy, Debug)]
pub enum Error<'a> {
    /// Expected these tokens
    Expected(&'static [Token]),
    /// Used `//~^^^` with too many carets, and the adjusted line doesn't exist
    LineDoesntExist,
    /// Used `//~|`, but there is no annotation in the previous line
    NoPrecedingAnnotation,
    /// Unknown compiler message `kind`
    UnknownKind(&'a str),
    /// No token starts with this character
    UnknownStartOfToken(char),
}

impl<'a> fmt::Display for Error<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Expected(toks) => {
                match toks {
                    [tok] => write!(f, "expected token `{}`", tok),
                    toks => {
                        try!(f.write_str("expected one of "));

                        let mut is_first = true;

                        for &tok in toks {
                            if is_first {
                                is_first = false;
                                try!(write!(f, "`{}`", tok))
                            } else {
                                try!(write!(f, ", `{}`", tok))
                            }
                        }

                        Ok(())
                    }
                }
            },
            Error::LineDoesntExist => f.write_str("adjusted line doesn't exist"),
            Error::NoPrecedingAnnotation => f.write_str("no annotation in previous line"),
            Error::UnknownKind(k) => write!(f, "unknown kind `{}`", k),
            Error::UnknownStartOfToken(c) => write!(f, "unknown start of token `{}`", c),
        }
    }
}

/// Formats parser errors into human readable messages
pub fn format_error(path: &Path, source: &str, span: Span, e: Error) -> String {
    let Span(start, end) = span;

    let mut ln = 1;
    let mut start_of_line = 0;
    for line in source.lines() {
        let length = line.len();

        if start_of_line <= start && start <= start_of_line + length {
            let start = start - start_of_line;
            let end = end - start_of_line;
            let ln = ln.to_string();

            let path = path.to_string_lossy();
            let mut error = format!("{path}:{line}:{start}: {line}:{end} error: {message}\n",
                                    path = path,
                                    line = ln,
                                    start = start.to_string(),
                                    end = end.to_string(),
                                    message = e.to_string());
            error.push_str(&format!("{path}:{line} {source}\n",
                                    path = path,
                                    line = ln,
                                    source = line));
            let ws =
                UnicodeWidthStr::width(&*path) +
                UnicodeWidthStr::width(":") +
                UnicodeWidthStr::width(&*ln) +
                UnicodeWidthStr::width(" ") +
                UnicodeWidthStr::width(&line[..start]);
            let span = UnicodeWidthStr::width(&line[start..end]).checked_sub(1).unwrap_or(0);
            error.push_str(&format!("{whitespace}^{span}",
                                    whitespace = iter::repeat(' ').take(ws).collect::<String>(),
                                    span = iter::repeat('~').take(span).collect::<String>()));

            return error
        }

        start_of_line += length + "\n".len();
        ln += 1;
    }

    // NB we always have *one* error that will be formatted while scanning the lines of the source
    // code. That formatted string will be returned as soon as the error is found, therefore this
    // part is unreachable
    unreachable!();
}

/// A `cfail` annotation parser.
///
/// Annotations can take any of the following forms:
///
/// - An inline annotation, the compiler message points to this line
///
/// ``` text
/// 0.foo();  //~ <kind> <message>
/// ```
///
/// - An adjusted annotation, the compiler message points to a line that's `adjust` lines above
///   this one, where `adjust` is the number of `^`s.
///
/// ``` text
/// 0.foo();
/// //~^ <kind> <message>
/// ```
///
/// - A multi-line annotation.
///
/// ``` text
/// let _: i8 = 0u8;
/// //~^ <kind> <message>
/// //~| <continuation of message>
/// //~| <continuation of message>
/// ```
///
/// - Shared annotations. All these annotations share the same line number.
///
/// ``` text
/// 0.count_zeros();
/// //~^ <kind> <message>
/// //~| <kind> <message>
/// //~| <kind> <message>
/// ```
pub struct Parser<'a> {
    curr_line: Line,
    last_line: Option<usize>,
    last_match: Option<Line>,
    lines: Peekable<Lines<'a>>,
    start_of_line: BytePos,
    state: Result<(), ()>,
}

impl<'a> Parser<'a> {
    /// Creates a parser for this source file
    pub fn new(source: &'a str) -> Parser<'a> {
        Parser {
            curr_line: Line(0),
            last_line: None,
            last_match: None,
            lines: source.lines().peekable(),
            start_of_line: 0,
            state: Ok(()),
        }
    }

    fn fatal<T>(&mut self, span: Span, e: Error<'a>) -> Option<Result<T, (Span, Error<'a>)>> {
        self.state = Err(());
        Some(Err((span + self.start_of_line, e)))
    }

    fn next_line(&mut self) -> Option<&'a str> {
        self.lines.next().map(|line| {
            if let Some(len) = self.last_line {
                self.start_of_line += len + "\n".len();
            }
            self.last_line = Some(line.len());
            self.curr_line = self.curr_line + 1;

            line
        })
    }
}

impl<'a> Iterator for Parser<'a> {
    type Item = Result<(Line, Kind, Cow<'a, str>), (Span, Error<'a>)>;

    fn next(&mut self) -> Option<Result<(Line, Kind, Cow<'a, str>), (Span, Error<'a>)>> {
        // Any kind
        const ANY: Kind = Kind::Error;
        const CARET_WS: &'static [Token] = &[Token::Caret, Token::Whitespace];
        const COLON_OR_WS: &'static [Token] = &[Token::Caret, Token::Or, Token::Whitespace];
        const COLON_WS: &'static [Token] = &[Token::Colon, Token::Whitespace];
        const K: &'static [Token] = &[Token::Kind(ANY)];
        const START: &'static str = "//~";

        if let Err(_) = self.state {
            return None
        }

        while let Some(line) = self.next_line() {
            if let Some(pos) = line.find(START) {
                let start = pos + START.len();
                let mut lexer = Lexer::new(&line[start..], start).peekable();

                let ln = match lexer.next() {
                    None => {
                        let span = Span(start, start);
                        return self.fatal(span, Error::Expected(COLON_OR_WS))
                    },
                    Some((span, Err(e))) => return self.fatal(span, e),
                    // adjusted annotation
                    Some((span, Ok(Token::Caret))) => {
                        let mut adj = 1;

                        loop {
                            match lexer.next() {
                                Some((_, Ok(Token::Caret))) => adj += 1,
                                Some((_, Ok(Token::Whitespace))) => break,
                                Some((span, Err(e))) => return self.fatal(span, e),
                                Some((span, Ok(_))) => {
                                    return self.fatal(span, Error::Expected(CARET_WS))
                                },
                                None => {
                                    let start = match lexer.peek() {
                                        None => line.len(),
                                        Some(&(span, _)) => span.0,
                                    };
                                    let span = Span(start, start);

                                    return self.fatal(span, Error::Expected(CARET_WS))
                                }
                            }
                        }

                        if let Some(ln) = self.curr_line - adj {
                            ln
                        } else {
                            return self.fatal(span, Error::LineDoesntExist)
                        }
                    },
                    // shared annotation
                    Some((span, Ok(Token::Or))) => {
                        if let Some(ln) = self.last_match {
                            ln
                        } else {
                            return self.fatal(span, Error::NoPrecedingAnnotation)
                        }
                    },
                    // inline annotation
                    Some((_, Ok(Token::Whitespace))) => self.curr_line,
                    Some((span, Ok(_))) => return self.fatal(span, Error::Expected(COLON_OR_WS)),
                };

                // eat whitespaces
                while let Some(&(_, Ok(Token::Whitespace))) = lexer.peek() {
                    lexer.next();
                }

                // <kind>
                let kind = match lexer.next() {
                    Some((_, Ok(Token::Kind(kind)))) => kind,
                    Some((span, _)) => return self.fatal(span, Error::Expected(K)),
                    None => {
                        let start = match lexer.peek() {
                            None => line.len(),
                            Some(&(span, _)) => span.0,
                        };

                        return self.fatal(Span(start, start), Error::Expected(K))
                    },
                };

                // optional `:`
                match lexer.peek() {
                    Some(&(_, Ok(Token::Colon))) => {
                        lexer.next();
                    },
                    Some(&(_, Ok(Token::Whitespace))) => {},
                    Some(&(span, _)) => {
                        return self.fatal(span, Error::Expected(COLON_WS))
                    },
                    None => {},
                }

                // eat whitespaces
                while let Some(&(_, Ok(Token::Whitespace))) = lexer.peek() {
                    lexer.next();
                }

                let start = match lexer.peek() {
                    None => line.len(),
                    Some(&(span, _)) => span.0,
                };

                self.last_match = Some(ln);

                let mut message = line[start..].into_cow();

                // check if the message is multi-line
                loop {
                    if let Some(line) = self.lines.peek() {
                        if let Some(pos) = line.find("//~|") {
                            const DUMMY: BytePos = 0;

                            let start = pos + "//~|".len();
                            let line = line[start..].trim();
                            let mut lexer = Lexer::new(line, DUMMY);

                            if let Some((_, Ok(Token::Kind(_)))) = lexer.next() {
                                // not multi-line
                                break
                            } else {
                                message.to_mut().push('\n');
                                message.to_mut().push_str(line);
                            }
                        } else {
                            break
                        }
                    } else {
                        break
                    }

                    self.next_line();
                }

                return Some(Ok((ln, kind, message)))
            } else {
                self.last_match = None;
                continue
            }
        }

        None
    }
}
