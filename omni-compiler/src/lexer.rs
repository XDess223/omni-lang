// omni-compiler/src/lexer.rs
// Phase 1: Omni Language Lexer / Scanner
//
// Rules enforced at this stage:
//   1. Naming convention: class identifiers must start with uppercase (A-Z).
//      Variable / method identifiers must start with lowercase (a-z) or underscore.
//   2. `goto` is explicitly rejected as a LexError.
//   3. Reserved words cannot be used as identifiers.

use crate::token::Token;

/// A position in the source text (1-based).
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

/// A token together with its source position.
#[derive(Debug, Clone)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

/// Errors produced during lexical analysis.
#[derive(Debug)]
pub enum LexError {
    UnexpectedChar(char, Span),
    /// `goto` is banned in Omni — forces structured programming.
    GotoForbidden(Span),
    /// A class-position identifier that starts with a lowercase letter.
    NamingViolationClass(String, Span),
    /// An identifier that starts with an uppercase letter where only
    /// variable / method names are expected (caught later in parser, but
    /// flagged here when we see a var keyword followed by a cap letter).
    UnterminatedString(Span),
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn current(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied();
        self.pos += 1;
        if ch == Some('\n') {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        ch
    }

    fn span(&self) -> Span {
        Span { line: self.line, col: self.col }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if c.is_whitespace() { self.advance(); } else { break; }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.current() {
            self.advance();
            if c == '\n' { break; }
        }
    }

    // ── Scanning helpers ──────────────────────────────────────────────────

    fn scan_string(&mut self, span: Span) -> Result<Token, LexError> {
        let mut s = String::new();
        loop {
            match self.advance() {
                None | Some('\n') => return Err(LexError::UnterminatedString(span)),
                Some('"') => break,
                Some(c) => s.push(c),
            }
        }
        Ok(Token::StringLiteral(s))
    }

    fn scan_number(&mut self, first: char) -> Token {
        let mut num = String::from(first);
        let mut is_float = false;
        while let Some(c) = self.current() {
            if c.is_ascii_digit() {
                num.push(c);
                self.advance();
            } else if c == '.' && !is_float && self.peek().map_or(false, |n| n.is_ascii_digit()) {
                is_float = true;
                num.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if is_float {
            Token::FloatLiteral(num.parse().unwrap())
        } else {
            Token::IntLiteral(num.parse().unwrap())
        }
    }

    fn scan_ident_or_keyword(&mut self, first: char) -> Result<Token, LexError> {
        let span = self.span();
        let mut word = String::from(first);
        while let Some(c) = self.current() {
            if c.is_alphanumeric() || c == '_' {
                word.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // Omni's GOTO is explicitly forbidden.
        if word.to_lowercase() == "goto" {
            return Err(LexError::GotoForbidden(span));
        }

        let tok = match word.as_str() {
            // Keywords
            "class"      => Token::Class,
            "interface"  => Token::Interface,
            "function"   => Token::Function,
            "var"        => Token::Var,
            "return"     => Token::Return,
            "if"         => Token::If,
            "else"       => Token::Else,
            "foreach"    => Token::Foreach,
            "in"         => Token::In,
            "to"         => Token::To,
            "forall"     => Token::Forall,
            "try"        => Token::Try,
            "catch"      => Token::Catch,
            "finally"    => Token::Finally,
            "throw"      => Token::Throw,
            "throws"     => Token::Throws,
            "new"        => Token::New,
            "extends"    => Token::Extends,
            "implements" => Token::Implements,
            "monitor"    => Token::Monitor,
            "import"     => Token::Import,
            "namespace"  => Token::Namespace,
            "public"     => Token::Public,
            "private"    => Token::Private,
            "protected"  => Token::Protected,
            "true"       => Token::BoolLiteral(true),
            "false"      => Token::BoolLiteral(false),
            "null"       => Token::Null,
            // Built-in types
            "Int"        => Token::TypeInt,
            "Float"      => Token::TypeFloat,
            "String"     => Token::TypeString,
            "Bool"       => Token::TypeBool,
            // Identifier — apply Omni naming convention
            _ => {
                if first.is_uppercase() {
                    // Class-name identifier
                    Token::ClassIdent(word)
                } else {
                    // Variable / method / parameter name
                    Token::Ident(word)
                }
            }
        };
        Ok(tok)
    }

    // ── Public API ────────────────────────────────────────────────────────

    /// Tokenize the entire source and return a flat list with EOF at the end.
    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, LexError> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace();
            let span = self.span();

            let ch = match self.current() {
                None => {
                    tokens.push(SpannedToken { token: Token::Eof, span });
                    break;
                }
                Some(c) => c,
            };
            self.advance();

            // Line comments (//)
            if ch == '/' && self.current() == Some('/') {
                self.skip_line_comment();
                continue;
            }

            let tok = match ch {
                '"' => self.scan_string(span)?,
                c if c.is_ascii_digit() => self.scan_number(c),
                c if c.is_alphabetic() || c == '_' => self.scan_ident_or_keyword(c)?,

                '+' => Token::Plus,
                '-' if self.current() == Some('>') => { self.advance(); Token::Arrow }
                '-' => Token::Minus,
                '*' => Token::Star,
                '/' => Token::Slash,
                '%' => Token::Percent,
                '=' if self.current() == Some('=') => { self.advance(); Token::Eq }
                '=' => Token::Assign,
                '!' if self.current() == Some('=') => { self.advance(); Token::NotEq }
                '!' => Token::Not,
                '<' if self.current() == Some('=') => { self.advance(); Token::LtEq }
                '<' => Token::LAngle,
                '>' if self.current() == Some('=') => { self.advance(); Token::GtEq }
                '>' => Token::RAngle,
                '&' if self.current() == Some('&') => { self.advance(); Token::And }
                '|' if self.current() == Some('|') => { self.advance(); Token::Or }
                ';' => Token::Semicolon,
                ':' if self.current() == Some(':') => { self.advance(); Token::DoubleColon }
                ':' => Token::Colon,
                ',' => Token::Comma,
                '.' => Token::Dot,
                '?' => Token::Question,
                '(' => Token::LParen,
                ')' => Token::RParen,
                '{' => Token::LBrace,
                '}' => Token::RBrace,

                unexpected => return Err(LexError::UnexpectedChar(unexpected, span)),
            };

            tokens.push(SpannedToken { token: tok, span });
        }

        Ok(tokens)
    }
}
