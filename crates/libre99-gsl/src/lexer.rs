// Modified MIT License
//
// Copyright (c) 2026 Joel Odom
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, and sublicense copies of the
// Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// "Commons Clause" License Condition v1.0
//
// The Software is provided to you by the Licensor under the License, subject to
// the following condition.
//
// Without limiting other conditions in the License, the grant of rights under the
// License will not include, and the License does not grant to you, the right to
// Sell the Software.
//
// For purposes of the foregoing, "Sell" means practicing any or all of the rights
// granted to you under the License to provide to third parties, for a fee or other
// consideration (including without limitation fees for hosting or consulting/
// support services related to the Software), a product or service whose value
// derives, entirely or substantially, from the functionality of the Software. Any
// license notice or attribution required by the License must also include this
// Commons Clause License Condition notice.
//
// Software: Libre99
//
// License: Modified MIT
//
// Licensor: Joel Odom
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! The GSL lexer: source text → a token stream with line numbers.
//!
//! Two things are special here:
//!
//! * `asm { … }` bodies are captured **verbatim** as [`Tok::AsmBody`] — raw
//!   lines handed to the `libre99gpl` assembler untouched (the block ends at
//!   the first line whose first non-blank character is `}`).
//! * `h` immediately followed by `>` or `<` lexes as the unsigned-compare
//!   operators (`h>`, `h>=`, `h<`, `h<=`) rather than an identifier — the one
//!   spelling quirk `docs/GSL.md` §5 documents.

/// A token with its 1-based source line.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub line: usize,
    pub tok: Tok,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    /// A numeric literal; `bare_zero` marks the exact token `0`.
    Num(i64, bool),
    Str(Vec<u8>),
    /// Raw `asm { … }` body lines, verbatim.
    AsmBody(Vec<String>),
    /// Punctuation / operators, by canonical spelling (`"=="`, `"h>="`, …).
    P(&'static str),
    Eof,
}

/// Lex errors carry the offending line.
#[derive(Debug, Clone)]
pub struct LexError {
    pub line: usize,
    pub message: String,
}

/// Tokenize a whole file.
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    Lexer { src: src.as_bytes(), pos: 0, line: 1, toks: Vec::new() }.run(src)
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
    toks: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn err(&self, message: impl Into<String>) -> LexError {
        LexError { line: self.line, message: message.into() }
    }

    fn peek(&self) -> u8 {
        *self.src.get(self.pos).unwrap_or(&0)
    }
    fn peek2(&self) -> u8 {
        *self.src.get(self.pos + 1).unwrap_or(&0)
    }
    fn bump(&mut self) -> u8 {
        let c = self.peek();
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
        }
        c
    }
    fn push(&mut self, tok: Tok) {
        self.toks.push(Token { line: self.line, tok });
    }

    fn run(mut self, src: &str) -> Result<Vec<Token>, LexError> {
        loop {
            self.skip_ws_and_comments()?;
            if self.pos >= self.src.len() {
                break;
            }
            let c = self.peek();
            match c {
                b'0'..=b'9' => self.number()?,
                b'\'' => self.char_lit()?,
                b'"' => self.string_lit()?,
                c if c == b'_' || c.is_ascii_alphabetic() => self.ident_or_asm(src)?,
                _ => self.punct()?,
            }
        }
        self.push(Tok::Eof);
        Ok(self.toks)
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            match self.peek() {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.bump();
                }
                b'/' if self.peek2() == b'/' => {
                    while self.pos < self.src.len() && self.peek() != b'\n' {
                        self.bump();
                    }
                }
                b'/' if self.peek2() == b'*' => {
                    let start = self.line;
                    self.bump();
                    self.bump();
                    loop {
                        if self.pos >= self.src.len() {
                            self.line = start;
                            return Err(self.err("unterminated /* comment"));
                        }
                        if self.peek() == b'*' && self.peek2() == b'/' {
                            self.bump();
                            self.bump();
                            break;
                        }
                        self.bump();
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn number(&mut self) -> Result<(), LexError> {
        let start = self.pos;
        if self.peek() == b'0' && (self.peek2() | 0x20) == b'x' {
            self.bump();
            self.bump();
            let ds = self.pos;
            while self.peek().is_ascii_hexdigit() {
                self.bump();
            }
            if self.pos == ds {
                return Err(self.err("malformed hex literal"));
            }
            let text = std::str::from_utf8(&self.src[ds..self.pos]).unwrap();
            let v = i64::from_str_radix(text, 16).map_err(|e| self.err(e.to_string()))?;
            self.push(Tok::Num(v, false));
        } else {
            while self.peek().is_ascii_digit() {
                self.bump();
            }
            let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
            let v: i64 = text.parse().map_err(|_| self.err("number out of range"))?;
            self.push(Tok::Num(v, text == "0"));
        }
        Ok(())
    }

    fn char_lit(&mut self) -> Result<(), LexError> {
        self.bump(); // '
        let c = self.bump();
        if c == 0 || c == b'\n' {
            return Err(self.err("unterminated character literal"));
        }
        if self.bump() != b'\'' {
            return Err(self.err("character literal must be a single character"));
        }
        self.push(Tok::Num(c as i64, false));
        Ok(())
    }

    fn string_lit(&mut self) -> Result<(), LexError> {
        self.bump(); // "
        let mut out = Vec::new();
        loop {
            match self.bump() {
                0 | b'\n' => return Err(self.err("unterminated string literal")),
                b'"' => break,
                b'\\' => match self.bump() {
                    b'\\' => out.push(b'\\'),
                    b'"' => out.push(b'"'),
                    b'x' => {
                        let hi = self.bump();
                        let lo = self.bump();
                        let h = (hi as char).to_digit(16);
                        let l = (lo as char).to_digit(16);
                        match (h, l) {
                            (Some(h), Some(l)) => out.push((h * 16 + l) as u8),
                            _ => return Err(self.err("\\x needs two hex digits")),
                        }
                    }
                    _ => return Err(self.err("unknown escape (use \\\\, \\\", \\xNN)")),
                },
                c => out.push(c),
            }
        }
        self.push(Tok::Str(out));
        Ok(())
    }

    fn ident_or_asm(&mut self, src: &str) -> Result<(), LexError> {
        // `h>` / `h>=` / `h<` / `h<=` — the unsigned-compare operators.
        if self.peek() == b'h' && matches!(self.peek2(), b'>' | b'<') {
            self.bump();
            let cmp = self.bump();
            let eq = self.peek() == b'=';
            if eq {
                self.bump();
            }
            let p = match (cmp, eq) {
                (b'>', false) => "h>",
                (b'>', true) => "h>=",
                (b'<', false) => "h<",
                (b'<', true) => "h<=",
                _ => unreachable!(),
            };
            self.push(Tok::P(p));
            return Ok(());
        }
        let start = self.pos;
        while {
            let c = self.peek();
            c == b'_' || c.is_ascii_alphanumeric()
        } {
            self.bump();
        }
        let name = &src[start..self.pos];
        if name == "asm" {
            // Peek: `asm` followed by `{` opens a verbatim block.
            let save = (self.pos, self.line);
            self.skip_ws_and_comments()?;
            if self.peek() == b'{' {
                self.push(Tok::Ident("asm".into()));
                self.bump(); // {
                return self.asm_body(src);
            }
            (self.pos, self.line) = save;
        }
        self.push(Tok::Ident(name.to_string()));
        Ok(())
    }

    /// Capture raw lines after `asm {` until a line starting (mod blanks) with `}`.
    fn asm_body(&mut self, src: &str) -> Result<(), LexError> {
        // The rest of the `{` line must be blank.
        while matches!(self.peek(), b' ' | b'\t' | b'\r') {
            self.bump();
        }
        if self.peek() != b'\n' {
            return Err(self.err("asm { must end its line (the body starts on the next line)"));
        }
        self.bump(); // newline
        let mut lines = Vec::new();
        loop {
            if self.pos >= self.src.len() {
                return Err(self.err("unterminated asm { } block"));
            }
            let start = self.pos;
            while self.pos < self.src.len() && self.peek() != b'\n' {
                self.pos += 1; // raw scan; line counter bumped below
            }
            let line = &src[start..self.pos];
            if self.pos < self.src.len() {
                self.pos += 1; // consume newline
            }
            self.line += 1;
            if line.trim_start().starts_with('}') {
                self.push(Tok::AsmBody(lines));
                return Ok(());
            }
            lines.push(line.trim_end().to_string());
        }
    }

    fn punct(&mut self) -> Result<(), LexError> {
        const THREE: &[&str] = &[">>>=", "<<=", ">>=", ">>>"];
        const TWO: &[&str] =
            &["==", "!=", "<=", ">=", "<<", ">>", "++", "--", "+=", "-=", "*=", "/=", "&=", "|=", "^="];
        const ONE: &[&str] = &[
            "{", "}", "(", ")", "[", "]", ";", ":", ",", "=", "+", "-", "*", "/", "%", "&", "|",
            "^", "~", "!", "<", ">", "@",
        ];
        let rest = &self.src[self.pos..];
        let starts = |p: &str| rest.starts_with(p.as_bytes());
        for p in THREE.iter().filter(|p| p.len() == 4).chain(THREE.iter().filter(|p| p.len() == 3))
        {
            if starts(p) {
                for _ in 0..p.len() {
                    self.bump();
                }
                self.push(Tok::P(p));
                return Ok(());
            }
        }
        for p in TWO {
            if starts(p) {
                self.bump();
                self.bump();
                self.push(Tok::P(p));
                return Ok(());
            }
        }
        for p in ONE {
            if starts(p) {
                self.bump();
                self.push(Tok::P(p));
                return Ok(());
            }
        }
        Err(self.err(format!("unexpected character '{}'", self.peek() as char)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn numbers_and_bare_zero() {
        assert_eq!(
            toks("0 0x00 12 'A'"),
            vec![
                Tok::Num(0, true),
                Tok::Num(0, false),
                Tok::Num(12, false),
                Tok::Num(65, false),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn unsigned_compare_ops_and_h_ident() {
        assert_eq!(
            toks("a h> b h() h>= c"),
            vec![
                Tok::Ident("a".into()),
                Tok::P("h>"),
                Tok::Ident("b".into()),
                Tok::Ident("h".into()),
                Tok::P("("),
                Tok::P(")"),
                Tok::P("h>="),
                Tok::Ident("c".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn comments_and_multichar_ops() {
        assert_eq!(
            toks("x >>>= 1; // c\n/* b */ y++"),
            vec![
                Tok::Ident("x".into()),
                Tok::P(">>>="),
                Tok::Num(1, false),
                Tok::P(";"),
                Tok::Ident("y".into()),
                Tok::P("++"),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn asm_blocks_are_verbatim() {
        let t = lex("asm {\nLBL  ST @>8300,>01\n* c\n}\n;").unwrap();
        assert_eq!(t[0].tok, Tok::Ident("asm".into()));
        match &t[1].tok {
            Tok::AsmBody(lines) => {
                assert_eq!(lines, &["LBL  ST @>8300,>01".to_string(), "* c".to_string()]);
            }
            other => panic!("expected AsmBody, got {other:?}"),
        }
        assert_eq!(t[2].tok, Tok::P(";"));
    }

    #[test]
    fn strings_with_escapes() {
        assert_eq!(toks(r#""AB\x41\"""#), vec![Tok::Str(vec![65, 66, 65, 34]), Tok::Eof]);
    }
}
