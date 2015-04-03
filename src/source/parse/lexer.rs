//! `cfail` annotation lexer

use std::fmt;
use std::iter::Peekable;
use std::str::CharIndices;

use source::parse::Error;
use {BytePos, Kind, Span};

/// Tokens found in `cfail` annotations
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Token {
    /// `^`
    Caret,
    /// `:`
    Colon,
    /// `error`
    Kind(Kind),
    /// `|`
    Or,
    /// ` `
    Whitespace,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Token::Caret => f.write_str("^"),
            Token::Colon => f.write_str(":"),
            Token::Kind(..) => f.write_str("<kind>"),
            Token::Or => f.write_str("|"),
            Token::Whitespace => f.write_str(" "),
        }
    }
}

/// EBNF:
///
/// ``` text
/// caret = "^" ;
/// colon = ":" ;
/// kind = "ERROR" | "HELP" | "NOTE" | "WARNING" | "error" | "help" | "note" | "warning" ;
/// or = "|" ;
/// whitespace = " " ;
/// ```
pub struct Lexer<'a> {
    input: &'a str,
    iter: Peekable<CharIndices<'a>>,
    offset: BytePos,
    state: Result<(), ()>,
}

impl<'a> Lexer<'a> {
    /// Creates a new lexer
    pub fn new(input: &'a str, offset: BytePos) -> Lexer<'a> {
        Lexer {
            input: input,
            iter: input.char_indices().peekable(),
            offset: offset,
            state: Ok(()),
        }
    }

    /// Raises a fatal error that terminates the lexing
    fn fatal<T>(&mut self, e: Error<'a>) -> Result<T, Error<'a>> {
        self.state = Err(());
        Err(e)
    }

    /// Returns the byte position of the next character
    fn next_byte_pos(&mut self) -> BytePos {
        match self.iter.peek() {
            None => self.input.len(),
            Some(&(i, _)) => i,
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = (Span, Result<Token, Error<'a>>);

    fn next(&mut self) -> Option<(Span, Result<Token, Error<'a>>)> {
        match self.state {
            Err(_) => None,
            Ok(_) => self.iter.next().map(|(i, c)| {
                macro_rules! spanned {
                    ($e:expr) => {
                        (Span(i + self.offset, self.next_byte_pos() + self.offset), $e)
                    }
                }

                let kind = match c {
                    ' ' => return spanned!(Ok(Token::Whitespace)),
                    ':' => return spanned!(Ok(Token::Colon)),
                    '^' => return spanned!(Ok(Token::Caret)),
                    '|' => return spanned!(Ok(Token::Or)),
                    'E' | 'e' => Kind::Error,
                    'H' | 'h' => Kind::Help,
                    'N' | 'n' => Kind::Note,
                    'W' | 'w' => Kind::Warning,
                    c => {
                        return spanned!(self.fatal(Error::UnknownStartOfToken(c)))
                    },
                };

                let needle = kind.needle();
                if self.input[i..].starts_with(needle) ||
                    self.input[i..].starts_with(&needle.to_uppercase())
                {
                    for _ in 0..needle.chars().count()-1 {
                        self.iter.next();
                    }

                    spanned!(Ok(Token::Kind(kind)))
                } else {
                    let end = if let Some(pos) = self.input[i..].find(" ") {
                        i + pos
                    } else {
                        self.input.len()
                    };

                    let span = Span(i + self.offset, end + self.offset);

                    (span, self.fatal(Error::UnknownKind(&self.input[i..end])))
                }
            }),
        }
    }
}
