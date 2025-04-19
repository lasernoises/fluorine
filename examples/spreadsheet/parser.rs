// This file is a heavily reduced version of some code from my cursed-lox project, which is a fork
// of https://github.com/Darksecond/lox.
// TODO: Figure out how to do attribution properly.

use std::{iter::Peekable, str::Chars};

#[derive(PartialEq, Debug, Clone)]
pub enum Token {
    // Single-character tokens.
    LeftParen,
    RightParen,
    Minus,
    Plus,
    Slash,
    Star,

    // Literals.
    /// with $
    Identifier(String),

    Number(f64),

    // Other.
    Eof,
    Unknown(char),
}

#[derive(PartialEq, Debug, Clone)]
enum TokenKind {
    LeftParen,
    RightParen,
    Minus,
    Plus,
    Slash,
    Star,
    Identifier,
    Number,
    Eof,
    Unknown,
}

impl From<&Token> for TokenKind {
    fn from(other: &Token) -> TokenKind {
        match other {
            Token::LeftParen => TokenKind::LeftParen,
            Token::RightParen => TokenKind::RightParen,
            Token::Minus => TokenKind::Minus,
            Token::Plus => TokenKind::Plus,
            Token::Slash => TokenKind::Slash,
            Token::Star => TokenKind::Star,
            Token::Identifier(_) => TokenKind::Identifier,
            Token::Number(_) => TokenKind::Number,
            Token::Eof => TokenKind::Eof,
            Token::Unknown(_) => TokenKind::Unknown,
        }
    }
}

struct Scanner<'a> {
    current_position: usize,
    it: Peekable<Chars<'a>>,
}

impl<'a> Scanner<'a> {
    fn new(buf: &str) -> Scanner {
        Scanner {
            current_position: 0,
            it: buf.chars().peekable(),
        }
    }

    fn next(&mut self) -> Option<char> {
        let next = self.it.next();
        if let Some(c) = next {
            self.current_position += c.len_utf8();
        }
        next
    }

    fn peek(&mut self) -> Option<&char> {
        self.it.peek()
    }

    // Consume next char if it matches
    fn consume_if<F>(&mut self, x: F) -> bool
    where
        F: Fn(char) -> bool,
    {
        if let Some(&ch) = self.peek() {
            if x(ch) {
                self.next().unwrap();
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    // Consume next char if the next one after matches (so .3 eats . if 3 is numeric, for example)
    fn consume_if_next<F>(&mut self, x: F) -> bool
    where
        F: Fn(char) -> bool,
    {
        let mut it = self.it.clone();
        match it.next() {
            None => return false,
            _ => (),
        }

        if let Some(&ch) = it.peek() {
            if x(ch) {
                self.next().unwrap();
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    fn consume_while<F>(&mut self, x: F) -> Vec<char>
    where
        F: Fn(char) -> bool,
    {
        let mut chars: Vec<char> = Vec::new();
        while let Some(&ch) = self.peek() {
            if x(ch) {
                self.next().unwrap();
                chars.push(ch);
            } else {
                break;
            }
        }
        chars
    }
}

struct Lexer<'a> {
    it: Scanner<'a>,
}

impl<'a> Lexer<'a> {
    fn new(buf: &str) -> Lexer {
        Lexer {
            it: Scanner::new(buf),
        }
    }

    fn match_token(&mut self, ch: char) -> Option<Token> {
        match ch {
            ' ' => None,
            '/' => {
                if self.it.consume_if(|ch| ch == '/') {
                    self.it.consume_while(|ch| ch != '\n');
                    None
                } else {
                    Some(Token::Slash)
                }
            }
            '\n' => None,
            '\t' => None,
            '\r' => None,
            x if x.is_numeric() => self.number(x),
            '$' => self.identifier(),
            '(' => Some(Token::LeftParen),
            ')' => Some(Token::RightParen),
            '-' => Some(Token::Minus),
            '+' => Some(Token::Plus),
            '*' => Some(Token::Star),
            c => Some(Token::Unknown(c)),
        }
    }

    fn identifier(&mut self) -> Option<Token> {
        let mut identifier = String::new();
        let rest: String = self
            .it
            .consume_while(|a| a.is_ascii_alphanumeric() || a == '_')
            .into_iter()
            .collect();
        identifier.push_str(rest.as_str());
        Some(Token::Identifier(identifier))
    }

    fn number(&mut self, x: char) -> Option<Token> {
        let mut number = String::new();
        number.push(x);
        let num: String = self
            .it
            .consume_while(|a| a.is_numeric())
            .into_iter()
            .collect();
        number.push_str(num.as_str());
        if self.it.peek() == Some(&'.') && self.it.consume_if_next(|ch| ch.is_numeric()) {
            let num2: String = self
                .it
                .consume_while(|a| a.is_numeric())
                .into_iter()
                .collect();
            number.push('.');
            number.push_str(num2.as_str());
        }
        Some(Token::Number(number.parse::<f64>().unwrap()))
    }

    fn tokenize_with_context(&mut self) -> Vec<Token> {
        let mut tokens: Vec<Token> = Vec::new();
        loop {
            let ch = match self.it.next() {
                None => break,
                Some(c) => c,
            };
            if let Some(token) = self.match_token(ch) {
                tokens.push(token);
            }
        }
        tokens
    }
}

pub fn tokenize_with_context(buf: &str) -> Vec<Token> {
    let mut t = Lexer::new(buf);
    t.tokenize_with_context()
}

fn parse_expr(it: &mut Parser, precedence: Precedence) -> Result<Expr, ()> {
    let mut expr = parse_prefix(it)?;
    while !it.is_eof() {
        let next_precedence = Precedence::from(it.peek());
        if precedence >= next_precedence {
            break;
        }
        expr = parse_infix(it, expr)?;
    }
    Ok(expr)
}

fn parse_infix(it: &mut Parser, left: Expr) -> Result<Expr, ()> {
    match it.peek() {
        TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash => {
            parse_binary(it, left)
        }
        // TokenKind::LeftParen => parse_call(it, left),
        _ => {
            it.error();
            Err(())
        }
    }
}

fn parse_grouping(it: &mut Parser) -> Result<Expr, ()> {
    it.expect(TokenKind::LeftParen)?;
    let expr = parse_expr(it, Precedence::None)?;
    it.expect(TokenKind::RightParen)?;

    Ok(Expr::Grouping(Box::new(expr)))
}

fn parse_prefix(it: &mut Parser) -> Result<Expr, ()> {
    match it.peek() {
        TokenKind::Number | TokenKind::Identifier => parse_primary(it),
        TokenKind::Minus => parse_unary(it),
        TokenKind::LeftParen => parse_grouping(it),
        _ => {
            it.error();
            Err(())
        }
    }
}

fn parse_binary(it: &mut Parser, left: Expr) -> Result<Expr, ()> {
    let precedence = Precedence::from(it.peek());
    let operator = parse_binary_op(it)?;
    let right = parse_expr(it, precedence)?;
    Ok(Expr::Binary(Box::new(left), operator, Box::new(right)))
}

fn parse_unary(it: &mut Parser) -> Result<Expr, ()> {
    let operator = parse_unary_op(it)?;
    let right = parse_expr(it, Precedence::Unary)?;
    Ok(Expr::Unary(operator, Box::new(right)))
}

fn parse_unary_op(it: &mut Parser) -> Result<UnaryOperator, ()> {
    let tc = it.advance();
    match &tc {
        &Token::Minus => Ok(UnaryOperator::Minus),
        _ => {
            it.error();
            Err(())
        }
    }
}

fn parse_binary_op(it: &mut Parser) -> Result<BinaryOperator, ()> {
    let tc = it.advance();
    let operator = match &tc {
        &Token::Plus => BinaryOperator::Plus,
        &Token::Minus => BinaryOperator::Minus,
        &Token::Star => BinaryOperator::Star,
        &Token::Slash => BinaryOperator::Slash,
        _ => {
            it.error();
            return Err(());
        }
    };

    Ok(operator)
}

fn parse_primary(it: &mut Parser) -> Result<Expr, ()> {
    let tc = it.advance();
    match &tc {
        &Token::Number(n) => Ok(Expr::Number(*n)),
        &Token::Identifier(s) => Ok(Expr::Variable(s.clone())),
        _ => {
            it.error();
            Err(())
        }
    }
}

pub fn parse(it: &mut Parser) -> Result<Expr, ()> {
    parse_expr(it, Precedence::None)
}

pub struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
    error: bool,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        Parser {
            tokens,
            cursor: 0,
            error: false,
        }
    }

    fn error(&mut self) {
        self.error = true;
    }

    fn is_eof(&self) -> bool {
        self.check(TokenKind::Eof)
    }

    fn peek(&self) -> TokenKind {
        self.peek_token().into()
    }

    fn peek_token(&self) -> &'a Token {
        match self.tokens.get(self.cursor) {
            Some(t) => t,
            None => &Token::Eof,
        }
    }

    fn check(&self, match_token: TokenKind) -> bool {
        let token = self.peek();
        token == match_token
    }

    fn advance(&mut self) -> &'a Token {
        let token = self.tokens.get(self.cursor);
        if let Some(token) = token {
            self.cursor = self.cursor + 1;
            token
        } else {
            &Token::Eof
        }
    }

    fn expect(&mut self, expected: TokenKind) -> Result<&'a Token, ()> {
        let token = self.advance();
        if TokenKind::from(token) == expected {
            Ok(token)
        } else {
            self.error();
            Err(())
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Expr {
    Binary(Box<Expr>, BinaryOperator, Box<Expr>),
    Grouping(Box<Expr>),
    Number(f64),
    Unary(UnaryOperator, Box<Expr>),
    Variable(Identifier),
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum BinaryOperator {
    Slash,
    Star,
    Plus,
    Minus,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum UnaryOperator {
    Minus,
}

pub type Identifier = String;

#[allow(dead_code)]
#[derive(PartialEq, PartialOrd, Copy, Clone)]
enum Precedence {
    None,
    Term,   // + -
    Factor, // * /
    Unary,  // ! -
    Primary,
}

impl<'a> From<TokenKind> for Precedence {
    fn from(token: TokenKind) -> Precedence {
        match token {
            TokenKind::Plus | TokenKind::Minus => Precedence::Term,
            TokenKind::Star | TokenKind::Slash => Precedence::Factor,
            _ => Precedence::None,
        }
    }
}
