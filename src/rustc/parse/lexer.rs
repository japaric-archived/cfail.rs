//! A `rustc` stderr lexer

use std::iter::Peekable;
use std::str::CharIndices;

use {BytePos, Kind};

/// Tokens found in `rustc` stderr
#[derive(Debug, PartialEq)]
pub enum Token {
    /// `:`
    Colon,
    /// `error`
    Kind(Kind),
    /// `123`
    Number(u32),
    /// ` `
    Whitespace,
}

/// EBNF:
///
/// ``` text
/// colon                = ":" ;
/// digit                = "0" | digit excluding zero ;
/// digit excluding zero = "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
/// kind                 = "error" | "help" | "note" | "warning" ;
/// number               = digit excluding zero , { digit } ;
/// whitespace           = " " ;
/// ```
pub struct Lexer<'a> {
    input: &'a str,
    iter: Peekable<CharIndices<'a>>,
    state: Result<(), ()>,
}

impl<'a> Lexer<'a> {
    /// Creates a new lexer
    pub fn new(input: &'a str) -> Lexer<'a> {
        Lexer {
            input: input,
            iter: input.char_indices().peekable(),
            state: Ok(())
        }
    }

    /// Tries to eat this token
    pub fn eat(&mut self, tok: Token) -> Result<(), ()> {
        match self.next() {
            None => Err(()),
            Some(result) => match (try!(result), tok) {
                (Token::Colon, Token::Colon) => Ok(()),
                (Token::Kind(_), Token::Kind(_)) => Ok(()),
                (Token::Number(_), Token::Number(_)) => Ok(()),
                (Token::Whitespace, Token::Whitespace) => Ok(()),
                _ => Err(())
            },
        }
    }

    /// Returns the byte position of the next character
    pub fn next_byte_pos(&mut self) -> BytePos {
        match self.iter.peek() {
            Some(&(start, _)) => start,
            None => self.input.len(),
        }
    }

    fn error<T>(&mut self) -> Result<T, ()> {
        self.state = Err(());
        Err(())
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Token, ()>;

    fn next(&mut self) -> Option<Result<Token, ()>> {
        match self.state {
            Err(_) => None,
            Ok(_) => self.iter.next().map(|(i, c)| {
                let kind = match c {
                    ' ' => return Ok(Token::Whitespace),
                    ':' => return Ok(Token::Colon),
                    'e' => Kind::Error,
                    'h' => Kind::Help,
                    'n' => Kind::Note,
                    'w' => Kind::Warning,
                    c if c.is_digit(10) && c != '0' => {
                        let start = i;
                        let mut end = self.input.len();

                        while let Some(&(i, c)) = self.iter.peek() {
                            if c.is_digit(10) {
                                self.iter.next();
                            } else {
                                end = i;
                                break;
                            }
                        }

                        return Ok(Token::Number(self.input[start..end].parse().unwrap()))
                    },
                    _ => {
                        return self.error()
                    },
                };

                let needle = kind.needle();
                if self.input[i..].starts_with(needle) {
                    for _ in 0..needle.len()-1 {
                        self.iter.next();
                    }

                    Ok(Token::Kind(kind))
                } else {
                    self.error()
                }
            }),
        }
    }
}
