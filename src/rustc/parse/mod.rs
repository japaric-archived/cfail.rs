//! `rustc` stderr parser

use std::iter::Peekable;
use std::str::Lines;

use {BytePos, Error, Kind, Line};

use self::lexer::{Lexer, Token};

pub mod lexer;

/// `rustc` stderr parser
///
/// All the compiler messages have the form:
///
/// ``` text
/// <path>:<line>:<byteppos_start> <line>:<bytepos_end> <kind>: <message>
/// ```
///
/// where <message> may span multiple lines.
///
/// stderr also includes spans that look like this:
///
/// ``` text
/// <path>:<line> <source>
///                ^~~~~~
/// ```
///
/// These compiler spans will be ignored by the parser.
///
/// The stderr always ends with a:
///
/// ``` text
/// error: aborting due to <n> previous errors
/// ```
pub struct Parser<'a> {
    input: &'a str,
    last_line: Option<usize>,
    lines: Peekable<Lines<'a>>,
    path: &'a str,
    start_of_line: BytePos,
}

impl<'a> Parser<'a> {
    /// Creates a new parser for the compiler stderr
    pub fn new(stderr: &'a str, path: &'a str) -> Parser<'a> {
        Parser {
            input: stderr,
            last_line: None,
            lines: stderr.lines().peekable(),
            path: path,
            start_of_line: 0,
        }
    }

    fn peek_line(&mut self) -> Option<&'a str> {
        self.lines.peek().map(|&line| line)
    }

    fn next_line(&mut self) -> Option<&'a str> {
        self.lines.next().map(|line| {
            if let Some(len) = self.last_line {
                self.start_of_line += len + "\n".len();
            }
            self.last_line = Some(line.len());

            line
        })
    }
}

impl<'a> Iterator for Parser<'a> {
    type Item = Result<(Line, Kind, &'a str), Error>;

    fn next(&mut self) -> Option<Result<(Line, Kind, &'a str), Error>> {
        while let Some(line) = self.next_line() {
            // <path>
            let (ln, kind, offset) = if line.starts_with(self.path) {
                let mut lexer = Lexer::new(&line[self.path.len()..]);

                match (|| {
                    // Any number
                    const ANY: u32 = 0;

                    // <path>:
                    try!(lexer.eat(Token::Colon));

                    // <path>:<line>
                    let line = if let Some(Ok(Token::Number(line))) = lexer.next() {
                        Line(line)
                    } else {
                        return Err(())
                    };

                    match lexer.next() {
                        // <path>:<line>:
                        Some(Ok(Token::Colon)) => {},
                        // <path>:<line> ...
                        Some(Ok(Token::Whitespace)) => {
                            // this is a compiler span, ignore
                            return Ok(None)
                        },
                        _ => return Err(()),
                    }

                    // <path>:<line>:<bytepos_start>
                    try!(lexer.eat(Token::Number(ANY)));

                    // <path>:<line>:<bytepos_start>: <line>
                    try!(lexer.eat(Token::Colon));
                    try!(lexer.eat(Token::Whitespace));
                    try!(lexer.eat(Token::Number(ANY)));

                    // <path>:<line>:<bytepos_start>: <line>:<bytepos_end>
                    try!(lexer.eat(Token::Colon));
                    try!(lexer.eat(Token::Number(ANY)));

                    // <path>:<line>:<bytepos_start>: <line>:<bytepos_end> <kind>
                    try!(lexer.eat(Token::Whitespace));
                    let kind = if let Some(Ok(Token::Kind(kind))) = lexer.next() {
                        kind
                    } else {
                        return Err(())
                    };

                    // <path>:<line>:<bytepos_start>: <line>:<bytepos_end> <kind>: <message>
                    try!(lexer.eat(Token::Colon));
                    try!(lexer.eat(Token::Whitespace));
                    let offset = lexer.next_byte_pos();

                    Ok(Some((line, kind, self.path.len() + offset)))
                })() {
                    Err(_) => return Some(Err(Error::ParseStderr(line.to_string()))),
                    Ok(None) => continue,
                    Ok(Some(payload)) => payload,
                }
            } else {
                continue
            };

            // At this point we have already parsed:
            //   <path>:<line>:<bytepos_start>: <line>:<bytepos_end> <kind>:
            //                                                               ^~ start
            // and `start` is the absolute byte position of the start of the message
            let start = self.start_of_line + offset;

            // Next, we check if this is a multi-line message.
            //
            // A multi-line message ends in either of these conditions:
            //
            // - Next line is a compiler span.
            // - Next line is another compiler message.
            // - Next line is the summary line: "error: aborting due to ..."
            let mut curr_line = line;
            while let Some(next_line) = self.peek_line() {
                if next_line.starts_with(self.path) ||
                    next_line.starts_with("error: aborting due to ")
                {
                    let end = self.start_of_line+curr_line.len();
                    return Some(Ok((ln, kind, &self.input[start..end])))
                } else {
                    curr_line = next_line;
                    self.next_line();
                }
            }

            // A compiler message can't never be the last line of stderr, because the last line is
            // always the summary line, therefore this is unreachable.
            unreachable!();
        }

        None
    }
}
