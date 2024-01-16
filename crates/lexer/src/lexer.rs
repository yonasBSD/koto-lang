use crate::{Position, Span};
use std::{collections::VecDeque, iter::Peekable, ops::Range, str::Chars};
use unicode_width::UnicodeWidthChar;
use unicode_xid::UnicodeXID;

/// The tokens that can emerge from the lexer
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Token {
    Error,
    Whitespace,
    NewLine,
    CommentSingle,
    CommentMulti,
    Number,
    Id,
    Wildcard,

    StringStart { quote: StringQuote, raw: bool },
    StringEnd,
    StringLiteral,

    // Symbols
    At,
    Colon,
    Comma,
    Dollar,
    Dot,
    Ellipsis,
    Function,
    RoundOpen,
    RoundClose,
    SquareOpen,
    SquareClose,
    CurlyOpen,
    CurlyClose,
    Range,
    RangeInclusive,

    // operators
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,

    Assign,
    AddAssign,
    SubtractAssign,
    MultiplyAssign,
    DivideAssign,
    RemainderAssign,

    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,

    Pipe,

    // Keywords
    And,
    Break,
    Catch,
    Continue,
    Debug,
    Else,
    ElseIf,
    Export,
    False,
    Finally,
    For,
    From,
    If,
    Import,
    In,
    Loop,
    Match,
    Not,
    Null,
    Or,
    Return,
    Self_,
    Switch,
    Then,
    Throw,
    True,
    Try,
    Until,
    While,
    Yield,
}

impl Token {
    /// Returns true if the token should be counted as whitespace
    pub fn is_whitespace(&self) -> bool {
        use Token::*;
        matches!(self, Whitespace | CommentMulti | CommentSingle)
    }

    /// Returns true if the token should be counted as whitespace, including newlines
    pub fn is_whitespace_including_newline(&self) -> bool {
        self.is_whitespace() || *self == Token::NewLine
    }
}

/// The type of quotation mark used in string delimiters
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum StringQuote {
    Double,
    Single,
}

impl TryFrom<char> for StringQuote {
    type Error = ();

    fn try_from(c: char) -> Result<Self, Self::Error> {
        match c {
            '"' => Ok(Self::Double),
            '\'' => Ok(Self::Single),
            _ => Err(()),
        }
    }
}

// Used to keep track of different lexing modes while working through a string
#[derive(Clone)]
enum StringMode {
    // Inside a string literal, expecting an end quote or the start of a template expression
    Literal(StringQuote),
    // Just after a $ symbol, either an id or a '{' will follow
    TemplateStart,
    // Inside a string template, e.g. '${...}'
    TemplateExpression,
    // Inside an inline map in a template expression, e.g. '${foo({bar: 42})}'
    // A closing '}' will end the map rather than the template expression.
    TemplateExpressionInlineMap,
    // The start of a raw string has just been consumed, raw string contents follow
    RawStart(StringQuote),
    // The contents of the raw string have just been consumed, the end delimiter should follow
    RawEnd(StringQuote),
}

// Separates the input source into Tokens
//
// TokenLexer is the internal implementation, KotoLexer provides the external interface.
#[derive(Clone)]
struct TokenLexer<'a> {
    // The input source
    source: &'a str,
    // The current position in the source
    current_byte: usize,
    // Used to provide the token's slice
    previous_byte: usize,
    // A cache of the previous token that was emitted
    previous_token: Option<Token>,
    // The span represented by the current token
    span: Span,
    // The indentation of the current line
    indent: usize,
    // A stack of string modes, allowing for nested mode changes while parsing strings
    string_mode_stack: Vec<StringMode>,
}

impl<'a> TokenLexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            previous_byte: 0,
            current_byte: 0,
            indent: 0,
            previous_token: None,
            span: Span::default(),
            string_mode_stack: vec![],
        }
    }

    fn source_bytes(&self) -> Range<usize> {
        self.previous_byte..self.current_byte
    }

    fn current_position(&self) -> Position {
        self.span.end
    }

    // Advance along the current line by a number of bytes
    //
    // The characters being advanced over should all be ANSI,
    // i.e. the byte count must match the character count.
    //
    // If the characters have been read as UTF-8 then advance_line_utf8 should be used instead.
    fn advance_line(&mut self, char_bytes: usize) {
        self.advance_line_utf8(char_bytes, char_bytes);
    }

    // Advance along the current line by a number of bytes, with a UTF-8 character count
    fn advance_line_utf8(&mut self, char_bytes: usize, char_count: usize) {
        // TODO, defer to advance_to_position
        self.previous_byte = self.current_byte;
        self.current_byte += char_bytes;

        let previous_end = self.span.end;
        self.span = Span {
            start: previous_end,
            end: Position {
                line: previous_end.line,
                column: previous_end.column + char_count as u32,
            },
        };
    }

    fn advance_to_position(&mut self, char_bytes: usize, position: Position) {
        self.previous_byte = self.current_byte;
        self.current_byte += char_bytes;

        self.span = Span {
            start: self.span.end,
            end: position,
        };
    }

    fn consume_newline(&mut self, mut chars: Peekable<Chars>) -> Token {
        use Token::*;

        let mut consumed_bytes = 1;

        if chars.peek() == Some(&'\r') {
            consumed_bytes += 1;
            chars.next();
        }

        match chars.next() {
            Some('\n') => {}
            _ => return Error,
        }

        self.advance_to_position(
            consumed_bytes,
            Position {
                line: self.current_position().line + 1,
                column: 1,
            },
        );

        NewLine
    }

    fn consume_comment(&mut self, mut chars: Peekable<Chars>) -> Token {
        use Token::*;

        // The # symbol has already been matched
        chars.next();

        if chars.peek() == Some(&'-') {
            // multi-line comment
            let mut char_bytes = 1;
            let mut position = self.current_position();
            position.column += 1;
            let mut end_found = false;
            while let Some(c) = chars.next() {
                char_bytes += c.len_utf8();
                position.column += c.width().unwrap_or(0) as u32;
                match c {
                    '#' => {
                        if chars.peek() == Some(&'-') {
                            chars.next();
                            char_bytes += 1;
                            position.column += 1;
                        }
                    }
                    '-' => {
                        if chars.peek() == Some(&'#') {
                            chars.next();
                            char_bytes += 1;
                            position.column += 1;
                            end_found = true;
                            break;
                        }
                    }
                    '\r' => {
                        if chars.next() != Some('\n') {
                            return Error;
                        }
                        char_bytes += 1;
                        position.line += 1;
                        position.column = 1;
                    }
                    '\n' => {
                        position.line += 1;
                        position.column = 1;
                    }
                    _ => {}
                }
            }

            self.advance_to_position(char_bytes, position);

            if end_found {
                CommentMulti
            } else {
                Error
            }
        } else {
            // single-line comment
            let (comment_bytes, comment_width) =
                consume_and_count_utf8(&mut chars, |c| !matches!(c, '\r' | '\n'));
            self.advance_line_utf8(comment_bytes + 1, comment_width + 1);
            CommentSingle
        }
    }

    fn consume_string_literal(&mut self, mut chars: Peekable<Chars>) -> Token {
        use Token::*;

        let end_quote = match self.string_mode_stack.last() {
            Some(StringMode::Literal(quote)) => *quote,
            _ => return Error,
        };

        let mut string_bytes = 0;
        let mut position = self.current_position();

        while let Some(c) = chars.peek().cloned() {
            match c {
                _ if c.try_into() == Ok(end_quote) => {
                    self.advance_to_position(string_bytes, position);
                    return StringLiteral;
                }
                '$' => {
                    self.advance_to_position(string_bytes, position);
                    return StringLiteral;
                }
                '\\' => {
                    chars.next();
                    string_bytes += 1;
                    position.column += 1;

                    let skip_next_char = match chars.peek() {
                        Some('$') => true,
                        Some('\\') => true,
                        Some(&c) if c.try_into() == Ok(end_quote) => true,
                        _ => false,
                    };

                    if skip_next_char {
                        chars.next();
                        string_bytes += 1;
                        position.column += 1;
                    }
                }
                '\r' => {
                    chars.next();
                    if chars.next() != Some('\n') {
                        return Error;
                    }
                    string_bytes += 2;
                    position.line += 1;
                    position.column = 1;
                }
                '\n' => {
                    chars.next();
                    string_bytes += 1;
                    position.line += 1;
                    position.column = 1;
                }
                _ => {
                    chars.next();
                    string_bytes += c.len_utf8();
                    position.column += c.width().unwrap_or(0) as u32;
                }
            }
        }

        Error
    }

    fn consume_raw_string_contents(
        &mut self,
        mut chars: Peekable<Chars>,
        end_quote: StringQuote,
    ) -> Token {
        let mut string_bytes = 0;

        let mut position = self.current_position();

        while let Some(c) = chars.next() {
            match c {
                _ if c.try_into() == Ok(end_quote) => {
                    self.advance_to_position(string_bytes, position);
                    self.string_mode_stack.pop(); // StringMode::RawStart
                    self.string_mode_stack.push(StringMode::RawEnd(end_quote));
                    return Token::StringLiteral;
                }
                '\r' => {
                    if chars.next() != Some('\n') {
                        return Token::Error;
                    }
                    string_bytes += 2;
                    position.line += 1;
                    position.column = 1;
                }
                '\n' => {
                    string_bytes += 1;
                    position.line += 1;
                    position.column = 1;
                }
                _ => {
                    string_bytes += c.len_utf8();
                    position.column += c.width().unwrap_or(0) as u32;
                }
            }
        }

        Token::Error
    }

    fn consume_raw_string_end(
        &mut self,
        mut chars: Peekable<Chars>,
        end_quote: StringQuote,
    ) -> Token {
        match chars.next() {
            Some(c) if c.try_into() == Ok(end_quote) => {
                self.string_mode_stack.pop(); // StringMode::RawEnd
                self.advance_line(1);
                Token::StringEnd
            }
            _ => Token::Error,
        }
    }

    fn consume_number(&mut self, mut chars: Peekable<Chars>) -> Token {
        use Token::*;

        let has_leading_zero = chars.peek() == Some(&'0');
        let mut char_bytes = consume_and_count(&mut chars, is_digit);
        let mut allow_exponent = true;

        match chars.peek() {
            Some(&'b') if has_leading_zero && char_bytes == 1 => {
                chars.next();
                char_bytes += 1 + consume_and_count(&mut chars, is_binary_digit);
                allow_exponent = false;
            }
            Some(&'o') if has_leading_zero && char_bytes == 1 => {
                chars.next();
                char_bytes += 1 + consume_and_count(&mut chars, is_octal_digit);
                allow_exponent = false;
            }
            Some(&'x') if has_leading_zero && char_bytes == 1 => {
                chars.next();
                char_bytes += 1 + consume_and_count(&mut chars, is_hex_digit);
                allow_exponent = false;
            }
            Some(&'.') => {
                chars.next();

                match chars.peek() {
                    Some(c) if is_digit(*c) => {}
                    Some(&'e') => {
                        // lookahead to check that this isn't a function call starting with 'e'
                        // e.g. 1.exp()
                        let mut lookahead = chars.clone();
                        lookahead.next();
                        match lookahead.peek() {
                            Some(c) if is_digit(*c) => {}
                            Some(&'+' | &'-') => {}
                            _ => {
                                self.advance_line(char_bytes);
                                return Number;
                            }
                        }
                    }
                    _ => {
                        self.advance_line(char_bytes);
                        return Number;
                    }
                }

                char_bytes += 1 + consume_and_count(&mut chars, is_digit);
            }
            _ => {}
        }

        if chars.peek() == Some(&'e') && allow_exponent {
            chars.next();
            char_bytes += 1;

            if matches!(chars.peek(), Some(&'+' | &'-')) {
                chars.next();
                char_bytes += 1;
            }

            char_bytes += consume_and_count(&mut chars, is_digit);
        }

        self.advance_line(char_bytes);
        Number
    }

    fn consume_id_or_keyword(&mut self, mut chars: Peekable<Chars>) -> Token {
        use Token::*;

        // The first character has already been matched
        let c = chars.next().unwrap();

        let (char_bytes, char_count) = consume_and_count_utf8(&mut chars, is_id_continue);
        let char_bytes = c.len_utf8() + char_bytes;
        let char_count = 1 + char_count;

        let id = &self.source[self.current_byte..self.current_byte + char_bytes];

        match id {
            "else" => {
                if self
                    .source
                    .get(self.current_byte..self.current_byte + char_bytes + 3)
                    == Some("else if")
                {
                    self.advance_line(7);
                    return ElseIf;
                } else {
                    self.advance_line(4);
                    return Else;
                }
            }
            "r" => {
                // look ahead and determine if this is the start of a raw string
                if let Some(&c) = chars.peek() {
                    if let Ok(quote) = c.try_into() {
                        self.advance_line(2);
                        self.string_mode_stack.push(StringMode::RawStart(quote));
                        return StringStart { quote, raw: true };
                    }
                }
            }
            _ => {}
        }

        macro_rules! check_keyword {
            ($keyword:expr, $token:ident) => {
                if id == $keyword {
                    self.advance_line($keyword.len());
                    return $token;
                }
            };
        }

        if !matches!(self.previous_token, Some(Token::Dot)) {
            check_keyword!("and", And);
            check_keyword!("break", Break);
            check_keyword!("catch", Catch);
            check_keyword!("continue", Continue);
            check_keyword!("debug", Debug);
            check_keyword!("export", Export);
            check_keyword!("false", False);
            check_keyword!("finally", Finally);
            check_keyword!("for", For);
            check_keyword!("from", From);
            check_keyword!("if", If);
            check_keyword!("import", Import);
            check_keyword!("in", In);
            check_keyword!("loop", Loop);
            check_keyword!("match", Match);
            check_keyword!("not", Not);
            check_keyword!("null", Null);
            check_keyword!("or", Or);
            check_keyword!("return", Return);
            check_keyword!("self", Self_);
            check_keyword!("switch", Switch);
            check_keyword!("then", Then);
            check_keyword!("throw", Throw);
            check_keyword!("true", True);
            check_keyword!("try", Try);
            check_keyword!("until", Until);
            check_keyword!("while", While);
            check_keyword!("yield", Yield);
        }

        // If no keyword matched, then consume as an Id
        self.advance_line_utf8(char_bytes, char_count);
        Token::Id
    }

    fn consume_wildcard(&mut self, mut chars: Peekable<Chars>) -> Token {
        // The _ has already been matched
        let c = chars.next().unwrap();

        let (char_bytes, char_count) = consume_and_count_utf8(&mut chars, is_id_continue);
        let char_bytes = c.len_utf8() + char_bytes;
        let char_count = 1 + char_count;

        self.advance_line_utf8(char_bytes, char_count);
        Token::Wildcard
    }

    fn consume_symbol(&mut self, remaining: &str) -> Option<Token> {
        use Token::*;

        macro_rules! check_symbol {
            ($token_str:expr, $token:ident) => {
                if remaining.starts_with($token_str) {
                    self.advance_line($token_str.len());
                    return Some($token);
                }
            };
        }

        check_symbol!("...", Ellipsis);

        check_symbol!("..=", RangeInclusive);
        check_symbol!("..", Range);

        check_symbol!(">>", Pipe);

        check_symbol!("==", Equal);
        check_symbol!("!=", NotEqual);
        check_symbol!(">=", GreaterOrEqual);
        check_symbol!("<=", LessOrEqual);
        check_symbol!(">", Greater);
        check_symbol!("<", Less);

        check_symbol!("=", Assign);
        check_symbol!("+=", AddAssign);
        check_symbol!("-=", SubtractAssign);
        check_symbol!("*=", MultiplyAssign);
        check_symbol!("/=", DivideAssign);
        check_symbol!("%=", RemainderAssign);

        check_symbol!("+", Add);
        check_symbol!("-", Subtract);
        check_symbol!("*", Multiply);
        check_symbol!("/", Divide);
        check_symbol!("%", Remainder);

        check_symbol!("@", At);
        check_symbol!(":", Colon);
        check_symbol!(",", Comma);
        check_symbol!(".", Dot);
        check_symbol!("(", RoundOpen);
        check_symbol!(")", RoundClose);
        check_symbol!("|", Function);
        check_symbol!("[", SquareOpen);
        check_symbol!("]", SquareClose);
        check_symbol!("{", CurlyOpen);
        check_symbol!("}", CurlyClose);

        None
    }

    fn get_next_token(&mut self) -> Option<Token> {
        use Token::*;

        let result = match self.source.get(self.current_byte..) {
            Some(remaining) if !remaining.is_empty() => {
                if self.previous_token == Some(Token::NewLine) {
                    // Reset the indent after a newline.
                    // If whitespace follows then the indent will be increased.
                    self.indent = 0;
                }

                let mut chars = remaining.chars().peekable();
                let next_char = *chars.peek().unwrap(); // At least one char is remaining

                let string_mode = self.string_mode_stack.last().cloned();

                let result = match string_mode {
                    Some(StringMode::Literal(quote)) => match next_char {
                        c if c.try_into() == Ok(quote) => {
                            self.advance_line(1);
                            self.string_mode_stack.pop();
                            StringEnd
                        }
                        '$' => {
                            self.advance_line(1);
                            self.string_mode_stack.push(StringMode::TemplateStart);
                            Dollar
                        }
                        _ => self.consume_string_literal(chars),
                    },
                    Some(StringMode::RawStart(quote)) => {
                        self.consume_raw_string_contents(chars, quote)
                    }
                    Some(StringMode::RawEnd(quote)) => self.consume_raw_string_end(chars, quote),
                    Some(StringMode::TemplateStart) => match next_char {
                        _ if is_id_start(next_char) => match self.consume_id_or_keyword(chars) {
                            Id => {
                                self.string_mode_stack.pop();
                                Id
                            }
                            _ => Error,
                        },
                        '{' => {
                            self.advance_line(1);
                            self.string_mode_stack.pop();
                            self.string_mode_stack.push(StringMode::TemplateExpression);
                            CurlyOpen
                        }
                        _ => Error,
                    },
                    _ => match next_char {
                        c if is_whitespace(c) => {
                            let count = consume_and_count(&mut chars, is_whitespace);
                            self.advance_line(count);
                            if matches!(self.previous_token, Some(Token::NewLine) | None) {
                                self.indent = count;
                            }
                            Whitespace
                        }
                        '\r' | '\n' => self.consume_newline(chars),
                        '#' => self.consume_comment(chars),
                        '"' => {
                            self.advance_line(1);
                            self.string_mode_stack
                                .push(StringMode::Literal(StringQuote::Double));
                            StringStart {
                                quote: StringQuote::Double,
                                raw: false,
                            }
                        }
                        '\'' => {
                            self.advance_line(1);
                            self.string_mode_stack
                                .push(StringMode::Literal(StringQuote::Single));
                            StringStart {
                                quote: StringQuote::Single,
                                raw: false,
                            }
                        }
                        '0'..='9' => self.consume_number(chars),
                        c if is_id_start(c) => self.consume_id_or_keyword(chars),
                        '_' => self.consume_wildcard(chars),
                        _ => {
                            let result = match self.consume_symbol(remaining) {
                                Some(result) => result,
                                None => {
                                    self.advance_line(1);
                                    Error
                                }
                            };

                            use StringMode::*;
                            match result {
                                CurlyOpen => {
                                    if matches!(string_mode, Some(TemplateExpression)) {
                                        self.string_mode_stack.push(TemplateExpressionInlineMap);
                                    }
                                }
                                CurlyClose => {
                                    if matches!(
                                        string_mode,
                                        Some(TemplateExpression | TemplateExpressionInlineMap)
                                    ) {
                                        self.string_mode_stack.pop();
                                    }
                                }
                                _ => {}
                            }

                            result
                        }
                    },
                };

                Some(result)
            }
            _ => None,
        };

        self.previous_token = result;
        result
    }
}

impl<'a> Iterator for TokenLexer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        self.get_next_token()
    }
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

fn is_binary_digit(c: char) -> bool {
    matches!(c, '0' | '1')
}

fn is_octal_digit(c: char) -> bool {
    matches!(c, '0'..='7')
}

fn is_hex_digit(c: char) -> bool {
    c.is_ascii_hexdigit()
}

fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t')
}

/// Returns true if the character matches the XID_Start Unicode property
pub fn is_id_start(c: char) -> bool {
    UnicodeXID::is_xid_start(c)
}

/// Returns true if the character matches the XID_Continue Unicode property
pub fn is_id_continue(c: char) -> bool {
    UnicodeXID::is_xid_continue(c)
}

fn consume_and_count(chars: &mut Peekable<Chars>, predicate: impl Fn(char) -> bool) -> usize {
    let mut char_bytes = 0;

    while let Some(c) = chars.peek() {
        if !predicate(*c) {
            break;
        }
        char_bytes += 1;
        chars.next();
    }

    char_bytes
}

fn consume_and_count_utf8(
    chars: &mut Peekable<Chars>,
    predicate: impl Fn(char) -> bool,
) -> (usize, usize) {
    let mut char_bytes = 0;
    let mut char_count = 0;

    while let Some(c) = chars.peek() {
        if !predicate(*c) {
            break;
        }
        char_bytes += c.len_utf8();
        char_count += c.width().unwrap_or(0);
        chars.next();
    }

    (char_bytes, char_count)
}

/// A [Token] along with additional metadata
#[derive(Clone, PartialEq, Debug)]
pub struct LexedToken {
    /// The token
    pub token: Token,
    /// The byte positions in the source representing the token
    pub source_bytes: Range<usize>,
    /// The token's span
    pub span: Span,
    /// The indentation level of the token's starting line
    pub indent: usize,
}

impl LexedToken {
    /// A helper for getting the token's starting line
    pub fn line(&self) -> u32 {
        self.span.start.line
    }

    /// A helper for getting the token's string slice from the source
    pub fn slice<'a>(&self, source: &'a str) -> &'a str {
        &source[self.source_bytes.clone()]
    }
}

impl Default for LexedToken {
    fn default() -> Self {
        Self {
            token: Token::Error,
            source_bytes: Default::default(),
            span: Default::default(),
            indent: Default::default(),
        }
    }
}

/// The lexer used by the Koto parser
///
/// Wraps a TokenLexer with unbounded lookahead, see peek_n().
#[derive(Clone)]
pub struct KotoLexer<'a> {
    lexer: TokenLexer<'a>,
    token_queue: VecDeque<LexedToken>,
}

impl<'a> KotoLexer<'a> {
    /// Initializes a lexer with the given input script
    pub fn new(source: &'a str) -> Self {
        Self {
            lexer: TokenLexer::new(source),
            token_queue: VecDeque::new(),
        }
    }

    /// Returns the input source
    pub fn source(&self) -> &'a str {
        self.lexer.source
    }

    /// Peeks the nth token that will appear in the output stream
    ///
    /// peek_n(0) is equivalent to calling peek().
    /// peek_n(1) returns the token that will appear after that, and so forth.
    pub fn peek(&mut self, n: usize) -> Option<&LexedToken> {
        let token_queue_len = self.token_queue.len();
        let tokens_to_add = token_queue_len + 1 - n.max(token_queue_len);

        for _ in 0..tokens_to_add {
            if let Some(next) = self.next_token() {
                self.token_queue.push_back(next);
            } else {
                break;
            }
        }

        self.token_queue.get(n)
    }

    fn next_token(&mut self) -> Option<LexedToken> {
        self.lexer.next().map(|token| LexedToken {
            token,
            source_bytes: self.lexer.source_bytes(),
            span: self.lexer.span,
            indent: self.lexer.indent,
        })
    }
}

impl<'a> Iterator for KotoLexer<'a> {
    type Item = LexedToken;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.token_queue.pop_front() {
            Some(next)
        } else {
            self.next_token()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod lexer_output {
        use super::{Token::*, *};

        fn check_lexer_output(source: &str, tokens: &[(Token, Option<&str>, u32)]) {
            let mut lex = KotoLexer::new(source);

            for (i, (token, maybe_slice, line_number)) in tokens.iter().enumerate() {
                loop {
                    match lex.next().expect("Expected token") {
                        LexedToken {
                            token: Whitespace, ..
                        } => continue,
                        output => {
                            assert_eq!(output.token, *token, "Token mismatch at position {i}");
                            if let Some(slice) = maybe_slice {
                                assert_eq!(
                                    output.slice(source),
                                    *slice,
                                    "Slice mismatch at position {i}"
                                );
                            }
                            assert_eq!(
                                output.line(),
                                *line_number,
                                "Line number mismatch at position {i}",
                            );
                            break;
                        }
                    }
                }
            }

            assert_eq!(lex.next(), None);
        }

        fn check_lexer_output_indented(source: &str, tokens: &[(Token, Option<&str>, u32, u32)]) {
            let mut lex = KotoLexer::new(source);

            for (i, (token, maybe_slice, line_number, indent)) in tokens.iter().enumerate() {
                loop {
                    match lex.next().expect("Expected token") {
                        LexedToken {
                            token: Whitespace, ..
                        } => continue,
                        output => {
                            assert_eq!(output.token, *token, "Mismatch at token {i}");
                            if let Some(slice) = maybe_slice {
                                assert_eq!(output.slice(source), *slice, "Mismatch at token {i}");
                            }
                            assert_eq!(
                                output.line(),
                                *line_number,
                                "Line number - expected: {}, actual: {} - (token {i} - {token:?})",
                                *line_number,
                                output.line(),
                            );
                            assert_eq!(
                                output.indent as u32, *indent,
                                "Indent (token {i} - {token:?})"
                            );
                            break;
                        }
                    }
                }
            }

            assert_eq!(lex.next(), None);
        }

        fn string_start(quote: StringQuote, raw: bool) -> Token {
            Token::StringStart { quote, raw }
        }

        #[test]
        fn ids() {
            let input = "id id1 id_2 i_d_3 ïd_ƒôûr if iff _ _foo";
            check_lexer_output(
                input,
                &[
                    (Id, Some("id"), 1),
                    (Id, Some("id1"), 1),
                    (Id, Some("id_2"), 1),
                    (Id, Some("i_d_3"), 1),
                    (Id, Some("ïd_ƒôûr"), 1),
                    (If, None, 1),
                    (Id, Some("iff"), 1),
                    (Wildcard, Some("_"), 1),
                    (Wildcard, Some("_foo"), 1),
                ],
            );
        }

        #[test]
        fn indent() {
            let input = "\
if true then
  foo 1

bar 2";
            check_lexer_output_indented(
                input,
                &[
                    (If, None, 1, 0),
                    (True, None, 1, 0),
                    (Then, None, 1, 0),
                    (NewLine, None, 1, 0),
                    (Id, Some("foo"), 2, 2),
                    (Number, Some("1"), 2, 2),
                    (NewLine, None, 2, 2),
                    (NewLine, None, 3, 0),
                    (Id, Some("bar"), 4, 0),
                    (Number, Some("2"), 4, 0),
                ],
            );
        }

        #[test]
        fn comments() {
            let input = "\
# single
true #-
multiline -
false #
-# true
()";
            check_lexer_output(
                input,
                &[
                    (CommentSingle, Some("# single"), 1),
                    (NewLine, None, 1),
                    (True, None, 2),
                    (CommentMulti, Some("#-\nmultiline -\nfalse #\n-#"), 2),
                    (True, None, 5),
                    (NewLine, None, 5),
                    (RoundOpen, None, 6),
                    (RoundClose, None, 6),
                ],
            );
        }

        #[test]
        fn strings() {
            let input = r#"
"hello, world!"
"escaped \\\"\n\$ string"
"double-\"quoted\" 'string'"
'single-\'quoted\' "string"'
""
"\\"
"#;

            use StringQuote::*;
            check_lexer_output(
                input,
                &[
                    (NewLine, None, 1),
                    (string_start(Double, false), None, 2),
                    (StringLiteral, Some("hello, world!"), 2),
                    (StringEnd, None, 2),
                    (NewLine, None, 2),
                    (string_start(Double, false), None, 3),
                    (StringLiteral, Some(r#"escaped \\\"\n\$ string"#), 3),
                    (StringEnd, None, 3),
                    (NewLine, None, 3),
                    (string_start(Double, false), None, 4),
                    (StringLiteral, Some(r#"double-\"quoted\" 'string'"#), 4),
                    (StringEnd, None, 4),
                    (NewLine, None, 4),
                    (string_start(Single, false), None, 5),
                    (StringLiteral, Some(r#"single-\'quoted\' "string""#), 5),
                    (StringEnd, None, 5),
                    (NewLine, None, 5),
                    (string_start(Double, false), None, 6),
                    (StringEnd, None, 6),
                    (NewLine, None, 6),
                    (string_start(Double, false), None, 7),
                    (StringLiteral, Some(r"\\"), 7),
                    (StringEnd, None, 7),
                    (NewLine, None, 7),
                ],
            );
        }

        #[test]
        fn raw_strings() {
            let input = r#"
r'$foo'
"#;

            check_lexer_output(
                input,
                &[
                    (NewLine, None, 1),
                    (string_start(StringQuote::Single, true), None, 2),
                    (StringLiteral, Some("$foo"), 2),
                    (StringEnd, None, 2),
                    (NewLine, None, 2),
                ],
            );
        }

        #[test]
        fn interpolated_string_ids() {
            let input = r#"
"hello $name, how are you?"
'$foo$bar'
"#;
            use StringQuote::*;
            check_lexer_output(
                input,
                &[
                    (NewLine, None, 1),
                    (string_start(Double, false), None, 2),
                    (StringLiteral, Some("hello "), 2),
                    (Dollar, None, 2),
                    (Id, Some("name"), 2),
                    (StringLiteral, Some(", how are you?"), 2),
                    (StringEnd, None, 2),
                    (NewLine, None, 2),
                    (string_start(Single, false), None, 3),
                    (Dollar, None, 3),
                    (Id, Some("foo"), 3),
                    (Dollar, None, 3),
                    (Id, Some("bar"), 3),
                    (StringEnd, None, 3),
                    (NewLine, None, 3),
                ],
            );
        }

        #[test]
        fn interpolated_string_expressions() {
            let input = r#"
"x + y == ${x + y}"
'${'{}'.format foo}'
"#;
            use StringQuote::*;
            check_lexer_output(
                input,
                &[
                    (NewLine, None, 1),
                    (string_start(Double, false), None, 2),
                    (StringLiteral, Some("x + y == "), 2),
                    (Dollar, None, 2),
                    (CurlyOpen, None, 2),
                    (Id, Some("x"), 2),
                    (Add, None, 2),
                    (Id, Some("y"), 2),
                    (CurlyClose, None, 2),
                    (StringEnd, None, 2),
                    (NewLine, None, 2),
                    (string_start(Single, false), None, 3),
                    (Dollar, None, 3),
                    (CurlyOpen, None, 3),
                    (string_start(Single, false), None, 3),
                    (StringLiteral, Some("{}"), 3),
                    (StringEnd, None, 3),
                    (Dot, None, 3),
                    (Id, Some("format"), 3),
                    (Id, Some("foo"), 3),
                    (CurlyClose, None, 3),
                    (StringEnd, None, 3),
                    (NewLine, None, 3),
                ],
            );
        }

        #[test]
        fn operators() {
            let input = "> >= >> < <=";

            check_lexer_output(
                input,
                &[
                    (Greater, None, 1),
                    (GreaterOrEqual, None, 1),
                    (Pipe, None, 1),
                    (Less, None, 1),
                    (LessOrEqual, None, 1),
                ],
            );
        }

        #[test]
        fn numbers() {
            let input = "\
123
55.5
-1e-3
0.5e+9
-8e8
0xabadcafe
0xABADCAFE
0o707606
0b1010101";
            check_lexer_output(
                input,
                &[
                    (Number, Some("123"), 1),
                    (NewLine, None, 1),
                    (Number, Some("55.5"), 2),
                    (NewLine, None, 2),
                    (Subtract, None, 3),
                    (Number, Some("1e-3"), 3),
                    (NewLine, None, 3),
                    (Number, Some("0.5e+9"), 4),
                    (NewLine, None, 4),
                    (Subtract, None, 5),
                    (Number, Some("8e8"), 5),
                    (NewLine, None, 5),
                    (Number, Some("0xabadcafe"), 6),
                    (NewLine, None, 6),
                    (Number, Some("0xABADCAFE"), 7),
                    (NewLine, None, 7),
                    (Number, Some("0o707606"), 8),
                    (NewLine, None, 8),
                    (Number, Some("0b1010101"), 9),
                ],
            );
        }

        #[test]
        fn lookups_on_numbers() {
            let input = "\
1.0.sin()
-1e-3.abs()
1.min x
9.exp()";
            check_lexer_output(
                input,
                &[
                    (Number, Some("1.0"), 1),
                    (Dot, None, 1),
                    (Id, Some("sin"), 1),
                    (RoundOpen, None, 1),
                    (RoundClose, None, 1),
                    (NewLine, None, 1),
                    (Subtract, None, 2),
                    (Number, Some("1e-3"), 2),
                    (Dot, None, 2),
                    (Id, Some("abs"), 2),
                    (RoundOpen, None, 2),
                    (RoundClose, None, 2),
                    (NewLine, None, 2),
                    (Number, Some("1"), 3),
                    (Dot, None, 3),
                    (Id, Some("min"), 3),
                    (Id, Some("x"), 3),
                    (NewLine, None, 3),
                    (Number, Some("9"), 4),
                    (Dot, None, 4),
                    (Id, Some("exp"), 4),
                    (RoundOpen, None, 4),
                    (RoundClose, None, 4),
                ],
            );
        }

        #[test]
        fn modify_assign() {
            let input = "\
a += 1
b -= 2
c *= 3";
            check_lexer_output(
                input,
                &[
                    (Id, Some("a"), 1),
                    (AddAssign, None, 1),
                    (Number, Some("1"), 1),
                    (NewLine, None, 1),
                    (Id, Some("b"), 2),
                    (SubtractAssign, None, 2),
                    (Number, Some("2"), 2),
                    (NewLine, None, 2),
                    (Id, Some("c"), 3),
                    (MultiplyAssign, None, 3),
                    (Number, Some("3"), 3),
                ],
            );
        }

        #[test]
        fn ranges() {
            let input = "\
a[..=9]
x = [i for i in 0..5]";
            check_lexer_output(
                input,
                &[
                    (Id, Some("a"), 1),
                    (SquareOpen, None, 1),
                    (RangeInclusive, None, 1),
                    (Number, Some("9"), 1),
                    (SquareClose, None, 1),
                    (NewLine, None, 1),
                    (Id, Some("x"), 2),
                    (Assign, None, 2),
                    (SquareOpen, None, 2),
                    (Id, Some("i"), 2),
                    (For, None, 2),
                    (Id, Some("i"), 2),
                    (In, None, 2),
                    (Number, Some("0"), 2),
                    (Range, None, 2),
                    (Number, Some("5"), 2),
                    (SquareClose, None, 2),
                ],
            );
        }

        #[test]
        fn function() {
            let input = "\
export f = |a, b...|
  c = a + b.size()
  c
f()";
            check_lexer_output_indented(
                input,
                &[
                    (Export, None, 1, 0),
                    (Id, Some("f"), 1, 0),
                    (Assign, None, 1, 0),
                    (Function, None, 1, 0),
                    (Id, Some("a"), 1, 0),
                    (Comma, None, 1, 0),
                    (Id, Some("b"), 1, 0),
                    (Ellipsis, None, 1, 0),
                    (Function, None, 1, 0),
                    (NewLine, None, 1, 0),
                    (Id, Some("c"), 2, 2),
                    (Assign, None, 2, 2),
                    (Id, Some("a"), 2, 2),
                    (Add, None, 2, 2),
                    (Id, Some("b"), 2, 2),
                    (Dot, None, 2, 2),
                    (Id, Some("size"), 2, 2),
                    (RoundOpen, None, 2, 2),
                    (RoundClose, None, 2, 2),
                    (NewLine, None, 2, 2),
                    (Id, Some("c"), 3, 2),
                    (NewLine, None, 3, 2),
                    (Id, Some("f"), 4, 0),
                    (RoundOpen, None, 4, 0),
                    (RoundClose, None, 4, 0),
                ],
            );
        }

        #[test]
        fn if_inline() {
            let input = "1 + if true then 0 else 1";
            check_lexer_output(
                input,
                &[
                    (Number, Some("1"), 1),
                    (Add, None, 1),
                    (If, None, 1),
                    (True, None, 1),
                    (Then, None, 1),
                    (Number, Some("0"), 1),
                    (Else, None, 1),
                    (Number, Some("1"), 1),
                ],
            );
        }

        #[test]
        fn if_block() {
            let input = "\
if true
  0
else if false
  1
else
  0";
            check_lexer_output_indented(
                input,
                &[
                    (If, None, 1, 0),
                    (True, None, 1, 0),
                    (NewLine, None, 1, 0),
                    (Number, Some("0"), 2, 2),
                    (NewLine, None, 2, 2),
                    (ElseIf, None, 3, 0),
                    (False, None, 3, 0),
                    (NewLine, None, 3, 0),
                    (Number, Some("1"), 4, 2),
                    (NewLine, None, 4, 2),
                    (Else, None, 5, 0),
                    (NewLine, None, 5, 0),
                    (Number, Some("0"), 6, 2),
                ],
            );
        }

        #[test]
        fn map_lookup() {
            let input = "m.检验.foo[1].bär()";

            check_lexer_output(
                input,
                &[
                    (Id, Some("m"), 1),
                    (Dot, None, 1),
                    (Id, Some("检验"), 1),
                    (Dot, None, 1),
                    (Id, Some("foo"), 1),
                    (SquareOpen, None, 1),
                    (Number, Some("1"), 1),
                    (SquareClose, None, 1),
                    (Dot, None, 1),
                    (Id, Some("bär"), 1),
                    (RoundOpen, None, 1),
                    (RoundClose, None, 1),
                ],
            );
        }

        #[test]
        fn map_lookup_with_keyword_as_key() {
            let input = "foo.and()";

            check_lexer_output(
                input,
                &[
                    (Id, Some("foo"), 1),
                    (Dot, None, 1),
                    (Id, Some("and"), 1),
                    (RoundOpen, None, 1),
                    (RoundClose, None, 1),
                ],
            );
        }

        #[test]
        fn windows_line_endings() {
            let input = "123\r\n456\r\n789";

            check_lexer_output(
                input,
                &[
                    (Number, Some("123"), 1),
                    (NewLine, None, 1),
                    (Number, Some("456"), 2),
                    (NewLine, None, 2),
                    (Number, Some("789"), 3),
                ],
            );
        }
    }

    mod peek {
        use super::*;

        #[test]
        fn lookup_in_list() {
            let source = "
[foo.bar]
";
            let mut lex = KotoLexer::new(source);
            assert_eq!(lex.peek(0).unwrap().token, Token::NewLine);
            assert_eq!(lex.peek(1).unwrap().token, Token::SquareOpen);
            assert_eq!(lex.peek(2).unwrap().token, Token::Id);
            assert_eq!(lex.peek(2).unwrap().slice(source), "foo");
            assert_eq!(lex.peek(3).unwrap().token, Token::Dot);
            assert_eq!(lex.peek(4).unwrap().token, Token::Id);
            assert_eq!(lex.peek(4).unwrap().slice(source), "bar");
            assert_eq!(lex.peek(5).unwrap().token, Token::SquareClose);
            assert_eq!(lex.peek(6).unwrap().token, Token::NewLine);
            assert_eq!(lex.peek(7), None);
        }

        #[test]
        fn multiline_lookup() {
            let source = "
x.iter()
  .skip 1
";
            let mut lex = KotoLexer::new(source);
            assert_eq!(lex.peek(0).unwrap().token, Token::NewLine);
            assert_eq!(lex.peek(1).unwrap().token, Token::Id);
            assert_eq!(lex.peek(1).unwrap().slice(source), "x");
            assert_eq!(lex.peek(2).unwrap().token, Token::Dot);
            assert_eq!(lex.peek(3).unwrap().token, Token::Id);
            assert_eq!(lex.peek(3).unwrap().slice(source), "iter");
            assert_eq!(lex.peek(4).unwrap().token, Token::RoundOpen);
            assert_eq!(lex.peek(5).unwrap().token, Token::RoundClose);
            assert_eq!(lex.peek(6).unwrap().token, Token::NewLine);
            assert_eq!(lex.peek(7).unwrap().token, Token::Whitespace);
            assert_eq!(lex.peek(8).unwrap().token, Token::Dot);
            assert_eq!(lex.peek(9).unwrap().token, Token::Id);
            assert_eq!(lex.peek(9).unwrap().slice(source), "skip");
            assert_eq!(lex.peek(10).unwrap().token, Token::Whitespace);
            assert_eq!(lex.peek(11).unwrap().token, Token::Number);
            assert_eq!(lex.peek(12).unwrap().token, Token::NewLine);
            assert_eq!(lex.peek(13), None);
        }
    }
}
