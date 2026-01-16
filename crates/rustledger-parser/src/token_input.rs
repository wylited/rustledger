//! Token-based input using chumsky's slice input.
//!
//! This module provides utilities for parsing pre-tokenized input using
//! chumsky. We store tokens in a Vec and parse from a slice, with spans
//! tracked separately.
//!
//! This module is a proof-of-concept for token-based parsing. The types
//! will be used when migrating the parser from character-based to token-based.

// Allow dead_code for now - this is a proof-of-concept that will be used later
#![allow(dead_code)]

use chumsky::prelude::*;

use crate::logos_lexer::{tokenize, Token};

/// A spanned token - a token paired with its byte offset span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedToken<'src> {
    /// The token.
    pub token: Token<'src>,
    /// Byte offset span (start, end).
    pub span: (usize, usize),
}

impl<'src> SpannedToken<'src> {
    /// Create a new spanned token.
    pub const fn new(token: Token<'src>, start: usize, end: usize) -> Self {
        Self {
            token,
            span: (start, end),
        }
    }
}

/// Tokenized input ready for parsing.
///
/// This struct owns the token vector and provides a slice view for parsing.
pub struct TokenizedInput<'src> {
    tokens: Vec<SpannedToken<'src>>,
    source_len: usize,
}

impl<'src> TokenizedInput<'src> {
    /// Create tokenized input from source code.
    pub fn new(source: &'src str) -> Self {
        let raw_tokens = tokenize(source);
        let tokens = raw_tokens
            .into_iter()
            .map(|(token, span)| SpannedToken::new(token, span.start, span.end))
            .collect();

        Self {
            tokens,
            source_len: source.len(),
        }
    }

    /// Get the tokens as a slice for parsing.
    pub fn as_slice(&self) -> &[SpannedToken<'src>] {
        &self.tokens
    }

    /// Get the source length (for EOI span).
    pub const fn source_len(&self) -> usize {
        self.source_len
    }
}

/// Type alias for the parser extra with our token type.
pub type TokenExtra<'src> = extra::Err<Rich<'src, SpannedToken<'src>>>;

/// Parse a token by kind, extracting the inner token.
#[macro_export]
macro_rules! token {
    ($pattern:pat => $result:expr) => {
        select! {
            SpannedToken { token: $pattern, .. } => $result
        }
    };
    ($pattern:pat) => {
        select! {
            SpannedToken { token: $pattern, .. } => ()
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logos_lexer::Token;

    #[test]
    fn test_tokenized_input_basic() {
        let source = "2024-01-15";
        let input = TokenizedInput::new(source);

        assert_eq!(input.tokens.len(), 1);
        assert!(matches!(input.tokens[0].token, Token::Date("2024-01-15")));
        assert_eq!(input.tokens[0].span, (0, 10));
    }

    #[test]
    fn test_tokenized_input_multiple() {
        let source = "open Assets:Bank USD";
        let input = TokenizedInput::new(source);

        assert_eq!(input.tokens.len(), 3);
        assert!(matches!(input.tokens[0].token, Token::Open));
        assert!(matches!(input.tokens[1].token, Token::Account(_)));
        assert!(matches!(input.tokens[2].token, Token::Currency(_)));
    }

    #[test]
    fn test_parse_any_token() {
        let source = "2024-01-15";
        let input = TokenizedInput::new(source);

        let parser = any::<_, TokenExtra<'_>>();
        let result = parser.parse(input.as_slice()).into_result();

        assert!(result.is_ok(), "Parse failed: {result:?}");
        let tok = result.unwrap();
        assert!(matches!(tok.token, Token::Date("2024-01-15")));
    }

    #[test]
    fn test_parse_collect_all() {
        let source = "open Assets:Bank USD";
        let input = TokenizedInput::new(source);

        let parser = any::<_, TokenExtra<'_>>().repeated().collect::<Vec<_>>();
        let result = parser.parse(input.as_slice()).into_result();

        assert!(result.is_ok(), "Parse failed: {result:?}");
        let tokens = result.unwrap();
        assert_eq!(tokens.len(), 3);
    }

    #[test]
    fn test_parse_select_date() {
        let source = "2024-01-15 open Assets:Bank";
        let input = TokenizedInput::new(source);

        // Use filter instead of select! to avoid type inference issues
        // Also ignore remaining tokens since we only want the first one
        let parser = any::<_, TokenExtra<'_>>()
            .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Date(_)))
            .map(|t: SpannedToken<'_>| {
                if let Token::Date(d) = t.token {
                    d.to_string()
                } else {
                    unreachable!()
                }
            })
            .then_ignore(any().repeated());

        let result = parser.parse(input.as_slice()).into_result();

        assert!(result.is_ok(), "Parse failed: {result:?}");
        assert_eq!(result.unwrap(), "2024-01-15");
    }

    #[test]
    fn test_parse_sequence() {
        let source = "open Assets:Bank USD";
        let input = TokenizedInput::new(source);

        // Use filter/map pattern instead of select! for clearer typing
        let open_kw = any::<_, TokenExtra<'_>>()
            .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Open))
            .to("open");

        let account = any::<_, TokenExtra<'_>>()
            .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Account(_)))
            .map(|t: SpannedToken<'_>| {
                if let Token::Account(a) = t.token {
                    a.to_string()
                } else {
                    unreachable!()
                }
            });

        let currency = any::<_, TokenExtra<'_>>()
            .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Currency(_)))
            .map(|t: SpannedToken<'_>| {
                if let Token::Currency(c) = t.token {
                    c.to_string()
                } else {
                    unreachable!()
                }
            });

        let parser = open_kw.then(account).then(currency);

        let result = parser.parse(input.as_slice()).into_result();
        assert!(result.is_ok(), "Parse failed: {result:?}");

        let ((kw, acc), curr) = result.unwrap();
        assert_eq!(kw, "open");
        assert_eq!(acc, "Assets:Bank");
        assert_eq!(curr, "USD");
    }

    #[test]
    fn test_parse_with_newlines_and_indent() {
        let source = "txn\n  Assets:Bank 100 USD";
        let input = TokenizedInput::new(source);

        let parser = any::<_, TokenExtra<'_>>().repeated().collect::<Vec<_>>();
        let result = parser.parse(input.as_slice()).into_result();

        assert!(result.is_ok(), "Parse failed: {result:?}");
        let tokens = result.unwrap();

        // Should have: Txn, Newline, Indent, Account, Number, Currency
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Txn)));
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Newline)));
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Indent(_))));
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Account(_))));
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Number(_))));
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Currency(_))));
    }

    #[test]
    fn test_span_preservation() {
        let source = "open Assets:Bank";
        let input = TokenizedInput::new(source);

        // Verify spans are preserved correctly
        assert_eq!(input.tokens[0].span, (0, 4)); // "open"
        assert_eq!(input.tokens[1].span, (5, 16)); // "Assets:Bank"
    }
}
