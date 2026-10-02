//! Rust source as the tokens `error_codes_scan` reads: identifiers, string
//! literals and single punctuation characters, each with its line and the
//! `fn` it sits in. Comments, numbers, lifetimes and character literals are
//! dropped, and a string literal is one token whatever it holds, so a brace,
//! a quote or `"code":` inside one is never read as code.

use std::ops::Range;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Tok {
    Ident(String),
    /// A string literal's text between its quotes, escapes as written.
    Str(String),
    Punct(char),
}

#[derive(Debug, Clone)]
pub(super) struct Token {
    pub(super) tok: Tok,
    pub(super) line: usize,
    /// The innermost named `fn` whose body holds it, empty outside every one;
    /// set by `name_fns`.
    pub(super) func: Rc<str>,
}

struct Lexer {
    chars: Vec<char>,
    at: usize,
    line: usize,
    out: Vec<Token>,
    outside: Rc<str>,
}

impl Lexer {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    /// Step past one character, counting the newlines stepped over.
    fn bump(&mut self) {
        if self.peek(0) == Some('\n') {
            self.line += 1;
        }
        self.at += 1;
    }

    fn push(&mut self, tok: Tok, line: usize) {
        let func = Rc::clone(&self.outside);
        self.out.push(Token { tok, line, func });
    }

    /// A string's text after its opening quote, `hashes` deep when raw.
    fn string(&mut self, raw: Option<usize>) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek(0) {
            if raw.is_none() && c == '\\' {
                text.push(c);
                self.bump();
            } else if c == '"' && raw.is_none_or(|h| (1..=h).all(|k| self.peek(k) == Some('#'))) {
                self.at += 1 + raw.unwrap_or(0);
                return text;
            }
            if let Some(c) = self.peek(0) {
                text.push(c);
            }
            self.bump();
        }
        text
    }

    fn word(&mut self) {
        let (start, line) = (self.at, self.line);
        while self
            .peek(0)
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            self.at += 1;
        }
        let word: String = self.chars[start..self.at].iter().collect();
        let prefix = matches!(word.as_str(), "r" | "b" | "br" | "c" | "cr");
        let raw = word.ends_with('r');
        let hashes = if raw {
            self.chars[self.at..]
                .iter()
                .take_while(|&&c| c == '#')
                .count()
        } else {
            0
        };
        // `r#type` is a raw identifier, not a raw string: no quote after `#`.
        if prefix && self.peek(hashes) == Some('"') {
            self.at += hashes + 1;
            let text = self.string(raw.then_some(hashes));
            self.push(Tok::Str(text), line);
        } else if self.chars[start].is_ascii_digit() {
            // a number: dropped
        } else {
            self.push(Tok::Ident(word), line);
        }
    }

    /// A character literal is skipped whole; a lifetime's quote alone.
    fn quote(&mut self) {
        if self.peek(1) == Some('\\') {
            self.at += 3;
            while self.peek(0).is_some_and(|c| c != '\'') {
                self.at += 1;
            }
            self.at += 1;
        } else if self.peek(2) == Some('\'') {
            self.at += 3;
        } else {
            self.at += 1;
        }
    }

    fn comment(&mut self) {
        if self.peek(1) == Some('/') {
            while self.peek(0).is_some_and(|c| c != '\n') {
                self.at += 1;
            }
        } else {
            while self.peek(0).is_some()
                && !(self.peek(0) == Some('*') && self.peek(1) == Some('/'))
            {
                self.bump();
            }
            self.at += 2;
        }
    }
}

pub(super) fn lex(text: &str) -> Vec<Token> {
    let mut lexer = Lexer {
        chars: text.chars().collect(),
        at: 0,
        line: 1,
        out: Vec::new(),
        outside: Rc::from(""),
    };
    while let Some(c) = lexer.peek(0) {
        let line = lexer.line;
        match c {
            '/' if matches!(lexer.peek(1), Some('/' | '*')) => lexer.comment(),
            '"' => {
                lexer.at += 1;
                let text = lexer.string(None);
                lexer.push(Tok::Str(text), line);
            }
            '\'' => lexer.quote(),
            c if c.is_alphanumeric() || c == '_' => lexer.word(),
            c if c.is_whitespace() => lexer.bump(),
            c => {
                lexer.push(Tok::Punct(c), line);
                lexer.at += 1;
            }
        }
    }
    lexer.out
}

pub(super) fn opens(t: &Token) -> bool {
    matches!(t.tok, Tok::Punct('(' | '[' | '{'))
}

pub(super) fn closes(t: &Token) -> bool {
    matches!(t.tok, Tok::Punct(')' | ']' | '}'))
}

/// The comma-separated items inside the group that opens at `open`, and the
/// index just past its close. A trailing comma adds no item.
pub(super) fn args(toks: &[Token], open: usize) -> (Vec<Range<usize>>, usize) {
    let (mut items, mut depth, mut start) = (Vec::new(), 0usize, open + 1);
    for (i, t) in toks.iter().enumerate().skip(open) {
        if opens(t) {
            depth += 1;
        } else if closes(t) {
            depth -= 1;
            if depth == 0 {
                if start < i {
                    items.push(start..i);
                }
                return (items, i + 1);
            }
        } else if depth == 1 && t.tok == Tok::Punct(',') {
            items.push(start..i);
            start = i + 1;
        }
    }
    (items, toks.len())
}

/// The string a lone literal names: `"x"`, `"x".to_owned()` (or `.into()`,
/// `.to_string()`), or either inside `Some(…)`.
pub(super) fn literal(toks: &[Token]) -> Option<String> {
    let toks: Vec<&Tok> = toks.iter().map(|t| &t.tok).collect();
    literal_of(&toks)
}

fn literal_of(toks: &[&Tok]) -> Option<String> {
    match toks {
        [
            Tok::Ident(some),
            Tok::Punct('('),
            inner @ ..,
            Tok::Punct(')'),
        ] if some == "Some" => literal_of(inner),
        [Tok::Str(s)] => Some(s.clone()),
        [
            Tok::Str(s),
            Tok::Punct('.'),
            Tok::Ident(m),
            Tok::Punct('('),
            Tok::Punct(')'),
        ] if matches!(m.as_str(), "to_owned" | "into" | "to_string") => Some(s.clone()),
        _ => None,
    }
}

/// An expression's tokens run together, `&refusal.code` for `& refusal . code`.
pub(super) fn render(toks: &[Token]) -> String {
    toks.iter()
        .map(|t| match &t.tok {
            Tok::Ident(s) => s.clone(),
            Tok::Str(s) => format!("\"{s}\""),
            Tok::Punct(c) => c.to_string(),
        })
        .collect()
}
