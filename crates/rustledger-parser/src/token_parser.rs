//! Token-based parser using Logos lexer + Chumsky.
//!
//! This module provides a parser that operates on pre-tokenized input,
//! using our Logos lexer for fast tokenization and Chumsky for parsing.
//!
//! # Architecture
//!
//! ```text
//! Source (&str) → Logos tokenize() → Vec<SpannedToken> → Chumsky parser → Directives
//! ```
//!
//! The key benefit is that tokenization is ~54x faster with Logos (SIMD-accelerated),
//! and token-level parsing is simpler than character-level parsing.

// Some helper functions are defined for future use
#![allow(dead_code)]

use chrono::NaiveDate;
use chumsky::prelude::*;
use rust_decimal::Decimal;
use std::str::FromStr;

use rustledger_core::{
    Amount, Balance, Close, Commodity, CostSpec, Custom, Directive, Document, Event,
    IncompleteAmount, InternedStr, MetaValue, Note, Open, Pad, Posting, Price, PriceAnnotation,
    Query, Transaction,
};

use crate::error::{ParseError, ParseErrorKind};
use crate::logos_lexer::{tokenize, Token};
use crate::span::{Span, Spanned};
use crate::ParseResult;

// ============================================================================
// Token Input Types
// ============================================================================

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

/// Type alias for parser extra with our token type.
type TokExtra<'src> = extra::Err<Rich<'src, SpannedToken<'src>>>;

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert raw tokens from lexer to `SpannedToken`s for parsing.
fn make_tokens(source: &str) -> Vec<SpannedToken<'_>> {
    tokenize(source)
        .into_iter()
        .map(|(token, span)| SpannedToken::new(token, span.start, span.end))
        .collect()
}

/// Get the byte span from a slice index span, using the token spans.
fn index_to_byte_span(tokens: &[SpannedToken<'_>], start_idx: usize, end_idx: usize) -> Span {
    if tokens.is_empty() {
        return Span::new(0, 0);
    }
    let start = if start_idx < tokens.len() {
        tokens[start_idx].span.0
    } else if !tokens.is_empty() {
        tokens.last().unwrap().span.1
    } else {
        0
    };
    let end = if end_idx > 0 && end_idx <= tokens.len() {
        tokens[end_idx - 1].span.1
    } else if !tokens.is_empty() {
        tokens.last().unwrap().span.1
    } else {
        0
    };
    Span::new(start, end)
}

// ============================================================================
// Token Matchers (Primitives)
// ============================================================================

/// Match a date token and extract the `NaiveDate`.
fn tok_date<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], NaiveDate, TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Date(_)))
        .try_map(|t: SpannedToken<'src>, span| {
            if let Token::Date(s) = t.token {
                // Parse YYYY-MM-DD or YYYY/MM/DD
                let parts: Vec<&str> = s.split(['-', '/']).collect();
                if parts.len() == 3 {
                    let y: i32 = parts[0]
                        .parse()
                        .map_err(|_| Rich::custom(span, "invalid year"))?;
                    let m: u32 = parts[1]
                        .parse()
                        .map_err(|_| Rich::custom(span, "invalid month"))?;
                    let d: u32 = parts[2]
                        .parse()
                        .map_err(|_| Rich::custom(span, "invalid day"))?;
                    NaiveDate::from_ymd_opt(y, m, d)
                        .ok_or_else(|| Rich::custom(span, "invalid date"))
                } else {
                    Err(Rich::custom(span, "invalid date format"))
                }
            } else {
                Err(Rich::custom(span, "expected date"))
            }
        })
}

/// Match a number token and extract the Decimal.
fn tok_number<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], Decimal, TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Number(_)))
        .try_map(|t: SpannedToken<'src>, span| {
            if let Token::Number(s) = t.token {
                // Remove commas for parsing
                let clean: String = s.chars().filter(|&c| c != ',').collect();
                Decimal::from_str(&clean).map_err(|_| Rich::custom(span, "invalid number"))
            } else {
                Err(Rich::custom(span, "expected number"))
            }
        })
}

/// Match a string token and extract the content (without quotes).
fn tok_string<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], String, TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::String(_)))
        .map(|t: SpannedToken<'src>| {
            if let Token::String(s) = t.token {
                // Remove quotes and handle escapes
                let inner = &s[1..s.len() - 1];
                let mut result = String::new();
                let mut chars = inner.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(&next) = chars.peek() {
                            chars.next();
                            match next {
                                'n' => result.push('\n'),
                                't' => result.push('\t'),
                                'r' => result.push('\r'),
                                '\\' => result.push('\\'),
                                '"' => result.push('"'),
                                _ => {
                                    result.push('\\');
                                    result.push(next);
                                }
                            }
                        }
                    } else {
                        result.push(c);
                    }
                }
                result
            } else {
                String::new()
            }
        })
}

/// Match an account token and extract the string.
fn tok_account<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], &'src str, TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Account(_)))
        .map(|t: SpannedToken<'src>| {
            if let Token::Account(s) = t.token {
                s
            } else {
                ""
            }
        })
}

/// Match a currency token and extract the string.
fn tok_currency<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], &'src str, TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Currency(_)))
        .map(|t: SpannedToken<'src>| {
            if let Token::Currency(s) = t.token {
                s
            } else {
                ""
            }
        })
}

/// Match a tag token and extract the string (without # prefix).
fn tok_tag<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], &'src str, TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Tag(_)))
        .map(|t: SpannedToken<'src>| {
            if let Token::Tag(s) = t.token {
                // Strip the leading '#'
                &s[1..]
            } else {
                ""
            }
        })
}

/// Match a link token and extract the string (without ^ prefix).
fn tok_link<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], &'src str, TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Link(_)))
        .map(|t: SpannedToken<'src>| {
            if let Token::Link(s) = t.token {
                // Strip the leading '^'
                &s[1..]
            } else {
                ""
            }
        })
}

/// Match a metadata key token and extract the key (without colon).
fn tok_meta_key<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], &'src str, TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::MetaKey(_)))
        .map(|t: SpannedToken<'src>| {
            if let Token::MetaKey(s) = t.token {
                // Remove trailing colon
                &s[..s.len() - 1]
            } else {
                ""
            }
        })
}

/// Match a specific keyword token.
macro_rules! tok_keyword {
    ($name:ident, $variant:ident) => {
        fn $name<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone
        {
            any()
                .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::$variant))
                .to(())
        }
    };
}

tok_keyword!(tok_txn, Txn);
tok_keyword!(tok_balance, Balance);
tok_keyword!(tok_open, Open);
tok_keyword!(tok_close, Close);
tok_keyword!(tok_commodity, Commodity);
tok_keyword!(tok_pad, Pad);
tok_keyword!(tok_event, Event);
tok_keyword!(tok_query, Query);
tok_keyword!(tok_note, Note);
tok_keyword!(tok_document, Document);
tok_keyword!(tok_price, Price);
tok_keyword!(tok_custom, Custom);
tok_keyword!(tok_option, Option_);
tok_keyword!(tok_include, Include);
tok_keyword!(tok_plugin, Plugin);
tok_keyword!(tok_pushtag, Pushtag);
tok_keyword!(tok_poptag, Poptag);
tok_keyword!(tok_pushmeta, Pushmeta);
tok_keyword!(tok_popmeta, Popmeta);
tok_keyword!(tok_true, True);
tok_keyword!(tok_false, False);

/// Match a newline token.
fn tok_newline<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone
{
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Newline))
        .to(())
}

/// Match any indent token (2+ spaces).
/// Beancount accepts any indentation level for metadata and postings.
fn tok_indent<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Indent(_) | Token::DeepIndent(_)))
        .to(())
}

/// Match a deep indent token (4+ spaces) - for posting metadata.
fn tok_deep_indent<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::DeepIndent(_)))
        .to(())
}

/// Match a comment token and ignore it.
fn tok_comment<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone
{
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Comment(_)))
        .to(())
}

/// Match a star token (*).
fn tok_star<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Star))
        .to(())
}

/// Match a pending token (!).
fn tok_pending<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone
{
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Pending))
        .to(())
}

/// Match any transaction flag and return the flag character.
fn tok_flag<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], char, TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| t.token.is_txn_flag())
        .map(|t: SpannedToken<'src>| match t.token {
            Token::Star => '*',
            Token::Pending => '!',
            Token::Flag(s) => s.chars().next().unwrap_or('?'),
            _ => '?',
        })
}

/// Match punctuation tokens.
macro_rules! tok_punct {
    ($name:ident, $variant:ident) => {
        fn $name<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone
        {
            any()
                .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::$variant))
                .to(())
        }
    };
}

tok_punct!(tok_lbrace, LBrace);
tok_punct!(tok_rbrace, RBrace);
tok_punct!(tok_ldoublebrace, LDoubleBrace);
tok_punct!(tok_rdoublebrace, RDoubleBrace);
tok_punct!(tok_lbracehash, LBraceHash);
tok_punct!(tok_lparen, LParen);
tok_punct!(tok_rparen, RParen);
tok_punct!(tok_at, At);
tok_punct!(tok_atat, AtAt);
tok_punct!(tok_comma, Comma);
tok_punct!(tok_tilde, Tilde);
tok_punct!(tok_plus, Plus);
tok_punct!(tok_minus, Minus);
tok_punct!(tok_slash, Slash);
tok_punct!(tok_colon, Colon);

// ============================================================================
// Compound Parsers
// ============================================================================

/// Parse an arithmetic expression with standard precedence.
fn tok_expr<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], Decimal, TokExtra<'src>> + Clone
{
    recursive(|expr| {
        // Atom: number or parenthesized expression
        let atom = choice((
            tok_lparen()
                .ignore_then(expr.clone())
                .then_ignore(tok_rparen()),
            tok_number(),
        ));

        // Unary: optional +/- prefix
        let unary = choice((tok_minus().to('-'), tok_plus().to('+')))
            .repeated()
            .collect::<Vec<_>>()
            .then(atom)
            .map(|(signs, n): (Vec<char>, Decimal)| {
                let neg_count = signs.iter().filter(|&&c| c == '-').count();
                if neg_count % 2 == 1 {
                    -n
                } else {
                    n
                }
            });

        // Term: unary combined with * and /
        let term = unary.clone().foldl(
            choice((tok_star().to('*'), tok_slash().to('/')))
                .then(unary)
                .repeated(),
            |left, (op, right)| {
                if op == '*' {
                    left * right
                } else {
                    left / right
                }
            },
        );

        // Expression: terms combined with + and -
        term.clone().foldl(
            choice((tok_plus().to('+'), tok_minus().to('-')))
                .then(term)
                .repeated(),
            |left, (op, right)| {
                if op == '+' {
                    left + right
                } else {
                    left - right
                }
            },
        )
    })
}

/// Parse an amount (number + currency).
fn tok_amount<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], Amount, TokExtra<'src>> + Clone {
    tok_expr()
        .then(tok_currency())
        .map(|(number, currency)| Amount::new(number, currency))
}

/// Parse an incomplete amount (for postings).
fn tok_incomplete_amount<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], IncompleteAmount, TokExtra<'src>> + Clone {
    choice((
        // Full amount: number + currency
        tok_expr()
            .then(tok_currency())
            .map(|(n, c)| IncompleteAmount::Complete(Amount::new(n, c))),
        // Number only
        tok_expr().map(IncompleteAmount::NumberOnly),
        // Currency only
        tok_currency().map(|c| IncompleteAmount::CurrencyOnly(c.into())),
    ))
}

/// A cost component - can be amount, number, currency, date, label, merge, or hash.
#[derive(Debug, Clone)]
enum TokCostComponent {
    /// Number + currency
    Amount(Decimal, String),
    /// Number only
    NumberOnly(Decimal),
    /// Currency only
    CurrencyOnly(String),
    /// Date
    Date(NaiveDate),
    /// String label
    Label(String),
    /// Merge marker (*)
    Merge,
    /// Hash separator (#) for per-unit/total syntax
    Hash,
}

/// Parse a hash token (# used as separator in cost specs).
fn tok_hash<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Flag("#")))
        .to(())
}

/// Parse a single cost component.
fn tok_cost_component<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], TokCostComponent, TokExtra<'src>> + Clone {
    choice((
        // Date (must come before number to avoid conflicts)
        tok_date().map(TokCostComponent::Date),
        // Amount (expr followed by currency) - use tok_expr() for arithmetic
        tok_expr()
            .then(tok_currency())
            .map(|(n, c)| TokCostComponent::Amount(n, c.to_string())),
        // Number only (expr for arithmetic)
        tok_expr().map(TokCostComponent::NumberOnly),
        // Currency only (must come after amount to avoid conflicts)
        tok_currency().map(|c| TokCostComponent::CurrencyOnly(c.to_string())),
        // String label
        tok_string().map(TokCostComponent::Label),
        // Merge marker
        tok_star().to(TokCostComponent::Merge),
        // Hash separator for per-unit # total syntax
        tok_hash().to(TokCostComponent::Hash),
    ))
}

/// Build a `CostSpec` from parsed components.
/// Handles the `#` syntax for combining per-unit and total costs:
/// - `{150 USD}` - per-unit cost
/// - `{{150 USD}}` - total cost
/// - `{150 # 5 USD}` - per-unit=150, total=5, currency=USD
/// - `{# 5 USD}` - total only
fn build_tok_cost_spec(components: Vec<TokCostComponent>, is_total_brace: bool) -> CostSpec {
    let mut spec = CostSpec::default();

    // Find if there's a hash separator
    let hash_pos = components
        .iter()
        .position(|c| matches!(c, TokCostComponent::Hash));

    // If hash present, components before # are per-unit, after # are total
    // If no hash, all components go to per-unit (or total if double-brace/brace-hash)
    let (per_unit_comps, total_comps): (Vec<_>, Vec<_>) = if let Some(pos) = hash_pos {
        let (before, after) = components.split_at(pos);
        (before.to_vec(), after[1..].to_vec()) // Skip the hash itself
    } else if is_total_brace {
        (vec![], components)
    } else {
        (components, vec![])
    };

    // Process per-unit components
    for comp in per_unit_comps {
        match comp {
            TokCostComponent::Amount(num, curr) => {
                spec.number_per = Some(num);
                spec.currency = Some(curr.into());
            }
            TokCostComponent::NumberOnly(num) => {
                spec.number_per = Some(num);
            }
            TokCostComponent::CurrencyOnly(curr) => {
                if spec.currency.is_none() {
                    spec.currency = Some(curr.into());
                }
            }
            TokCostComponent::Date(d) => {
                spec.date = Some(d);
            }
            TokCostComponent::Label(l) => {
                spec.label = Some(l);
            }
            TokCostComponent::Merge => {
                spec.merge = true;
            }
            TokCostComponent::Hash => {}
        }
    }

    // Process total components
    for comp in total_comps {
        match comp {
            TokCostComponent::Amount(num, curr) => {
                spec.number_total = Some(num);
                spec.currency = Some(curr.into());
            }
            TokCostComponent::NumberOnly(num) => {
                spec.number_total = Some(num);
            }
            TokCostComponent::CurrencyOnly(curr) => {
                if spec.currency.is_none() {
                    spec.currency = Some(curr.into());
                }
            }
            TokCostComponent::Date(d) => {
                spec.date = Some(d);
            }
            TokCostComponent::Label(l) => {
                spec.label = Some(l);
            }
            TokCostComponent::Merge => {
                spec.merge = true;
            }
            TokCostComponent::Hash => {}
        }
    }

    spec
}

/// Parse cost components with optional commas/slashes as delimiters.
/// Allows empty components: {, 100.0 USD, , }
fn tok_cost_components<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], Vec<TokCostComponent>, TokExtra<'src>> + Clone {
    // A delimiter is a comma or slash
    let delimiter = tok_comma().or(tok_slash()).to(());

    // Cost item: either a real component or a delimiter (to be skipped)
    let cost_item = choice((tok_cost_component().map(Some), delimiter.to(None)));

    // Parse items and filter out the None values (delimiters)
    cost_item
        .repeated()
        .collect::<Vec<_>>()
        .map(|items| items.into_iter().flatten().collect())
}

/// Parse a cost specification: { ... }, {{ ... }}, or {# ... }.
fn tok_cost_spec<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], CostSpec, TokExtra<'src>> + Clone {
    choice((
        // Total cost: {{ ... }} (legacy syntax)
        tok_ldoublebrace()
            .ignore_then(tok_cost_components())
            .then_ignore(tok_rdoublebrace())
            .map(|comps| build_tok_cost_spec(comps, true)),
        // Total cost: {# ... } (new syntax)
        tok_lbracehash()
            .ignore_then(tok_cost_components())
            .then_ignore(tok_rbrace())
            .map(|comps| build_tok_cost_spec(comps, true)),
        // Per-unit cost: { ... }
        tok_lbrace()
            .ignore_then(tok_cost_components())
            .then_ignore(tok_rbrace())
            .map(|comps| build_tok_cost_spec(comps, false)),
    ))
}

/// Parse a price annotation: @ [amount] or @@ [amount].
/// Amount can be missing for incomplete inputs.
fn tok_price_annotation<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], PriceAnnotation, TokExtra<'src>> + Clone {
    // Complete amount: expr + currency (use tok_expr() for arithmetic)
    let complete_amount = tok_expr()
        .then(tok_currency())
        .map(|(n, c)| Amount::new(n, c));

    // Incomplete amount: expr only or currency only
    let incomplete_amount = choice((
        tok_expr().map(IncompleteAmount::NumberOnly),
        tok_currency().map(|c| IncompleteAmount::CurrencyOnly(c.into())),
    ));

    // Price amount: complete, incomplete, or empty
    let _price_amount = choice((
        complete_amount.clone().map(Some),
        incomplete_amount.clone().map(Some),
    ));

    choice((
        // @@ with complete amount
        tok_atat()
            .ignore_then(complete_amount.clone())
            .map(PriceAnnotation::Total),
        // @@ with incomplete amount
        tok_atat()
            .ignore_then(incomplete_amount.clone())
            .map(PriceAnnotation::TotalIncomplete),
        // @@ with nothing (empty)
        tok_atat().to(PriceAnnotation::TotalEmpty),
        // @ with complete amount
        tok_at()
            .ignore_then(complete_amount)
            .map(PriceAnnotation::Unit),
        // @ with incomplete amount
        tok_at()
            .ignore_then(incomplete_amount)
            .map(PriceAnnotation::UnitIncomplete),
        // @ with nothing (empty)
        tok_at().to(PriceAnnotation::UnitEmpty),
    ))
}

/// Parse a boolean.
fn tok_boolean<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], bool, TokExtra<'src>> + Clone
{
    choice((tok_true().to(true), tok_false().to(false)))
}

/// Parse a metadata value.
fn tok_meta_value<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], MetaValue, TokExtra<'src>> + Clone {
    choice((
        tok_string().map(MetaValue::String),
        tok_boolean().map(MetaValue::Bool),
        tok_account().map(|s| MetaValue::Account(s.to_string())),
        tok_tag().map(|s| MetaValue::Tag(s.to_string())),
        tok_link().map(|s| MetaValue::Link(s.to_string())),
        tok_date().map(MetaValue::Date),
        tok_amount().map(MetaValue::Amount),
        // Use tok_expr() to allow arithmetic expressions in metadata values
        tok_expr().map(MetaValue::Number),
        tok_currency().map(|s| MetaValue::Currency(s.to_string())),
    ))
}

// ============================================================================
// Parsed Item Enum
// ============================================================================

/// Intermediate representation of parsed items.
#[derive(Debug, Clone)]
enum ParsedItem {
    Directive(Directive),
    Option(String, String),
    Include(String),
    Plugin(String, Option<String>),
    Pushtag(String),
    Poptag(String),
    Pushmeta(String, MetaValue),
    Popmeta(String),
    Comment,
}

// ============================================================================
// Directive Parsers
// ============================================================================

/// Parse an option directive.
fn tok_option_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], ParsedItem, TokExtra<'src>> {
    tok_option()
        .ignore_then(tok_string())
        .then(tok_string())
        .then_ignore(tok_comment().or_not())
        .map(|(key, value)| ParsedItem::Option(key, value))
}

/// Parse an include directive.
fn tok_include_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], ParsedItem, TokExtra<'src>> {
    tok_include()
        .ignore_then(tok_string())
        .then_ignore(tok_comment().or_not())
        .map(ParsedItem::Include)
}

/// Parse a plugin directive.
fn tok_plugin_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], ParsedItem, TokExtra<'src>> {
    tok_plugin()
        .ignore_then(tok_string())
        .then(tok_string().or_not())
        .then_ignore(tok_comment().or_not())
        .map(|(name, config)| ParsedItem::Plugin(name, config))
}

/// Parse a pushtag directive.
fn tok_pushtag_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], ParsedItem, TokExtra<'src>> {
    tok_pushtag()
        .ignore_then(tok_tag())
        .then_ignore(tok_comment().or_not())
        .map(|t| ParsedItem::Pushtag(t.to_string()))
}

/// Parse a poptag directive.
fn tok_poptag_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], ParsedItem, TokExtra<'src>> {
    tok_poptag()
        .ignore_then(tok_tag())
        .then_ignore(tok_comment().or_not())
        .map(|t| ParsedItem::Poptag(t.to_string()))
}

/// Parse a pushmeta directive.
fn tok_pushmeta_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], ParsedItem, TokExtra<'src>> {
    tok_pushmeta()
        .ignore_then(tok_meta_key())
        .then(tok_meta_value())
        .then_ignore(tok_comment().or_not())
        .map(|(key, value)| ParsedItem::Pushmeta(key.to_string(), value))
}

/// Parse a popmeta directive.
fn tok_popmeta_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], ParsedItem, TokExtra<'src>> {
    tok_popmeta()
        .ignore_then(tok_meta_key())
        .then_ignore(tok_comment().or_not())
        .map(|key| ParsedItem::Popmeta(key.to_string()))
}

/// Element that can appear in transaction header.
#[derive(Debug, Clone)]
enum TxnHeaderItem {
    String(String),
    Tag(String),
    Link(String),
}

/// Posting, metadata, or tag/link continuation.
#[derive(Debug, Clone)]
enum PostingOrMeta {
    Posting(Posting),
    Meta(String, MetaValue),
    TagsLinks(Vec<String>, Vec<String>),
}

/// Parse posting-level metadata (4+ spaces indent).
fn tok_posting_meta<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (String, MetaValue), TokExtra<'src>> + Clone {
    tok_newline()
        .ignore_then(tok_deep_indent())
        .ignore_then(tok_meta_key())
        .then(tok_meta_value().or_not())
        .then_ignore(tok_comment().or_not())
        .map(|(key, value)| (key.to_string(), value.unwrap_or(MetaValue::None)))
}

/// Parse a posting line with its metadata.
fn tok_posting_with_meta<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], Posting, TokExtra<'src>> + Clone {
    // Optional flag
    let flag = tok_flag().or_not();

    // Account is required
    let account = tok_account();

    // Amount is optional
    let amount = tok_incomplete_amount().or_not();

    // Cost spec is optional
    let cost = tok_cost_spec().or_not();

    // Price annotation is optional
    let price = tok_price_annotation().or_not();

    flag.then(account)
        .then(amount)
        .then(cost)
        .then(price)
        .then_ignore(tok_comment().or_not())
        .then(tok_posting_meta().repeated().collect::<Vec<_>>())
        .map(|(((((flag, account), amount), cost), price), metadata)| {
            // Create posting based on whether we have an amount
            let mut posting = if let Some(a) = amount {
                Posting::with_incomplete(account, a)
            } else {
                Posting::auto(account)
            };
            if let Some(f) = flag {
                posting = posting.with_flag(f);
            }
            if let Some(c) = cost {
                posting = posting.with_cost(c);
            }
            if let Some(p) = price {
                posting = posting.with_price(p);
            }
            // Add posting-level metadata
            for (key, value) in metadata {
                posting.meta.insert(key, value);
            }
            posting
        })
}

/// Parse a posting line (without consuming metadata, for use in `tok_posting_or_meta`).
fn tok_posting<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], Posting, TokExtra<'src>> + Clone {
    // Optional flag
    let flag = tok_flag().or_not();

    // Account is required
    let account = tok_account();

    // Amount is optional
    let amount = tok_incomplete_amount().or_not();

    // Cost spec is optional
    let cost = tok_cost_spec().or_not();

    // Price annotation is optional
    let price = tok_price_annotation().or_not();

    flag.then(account)
        .then(amount)
        .then(cost)
        .then(price)
        .then_ignore(tok_comment().or_not())
        .map(|((((flag, account), amount), cost), price)| {
            // Create posting based on whether we have an amount
            let mut posting = if let Some(a) = amount {
                Posting::with_incomplete(account, a)
            } else {
                Posting::auto(account)
            };
            if let Some(f) = flag {
                posting = posting.with_flag(f);
            }
            if let Some(c) = cost {
                posting = posting.with_cost(c);
            }
            if let Some(p) = price {
                posting = posting.with_price(p);
            }
            posting
        })
}

/// Parse a metadata line inside a directive, returning None for comment-only lines.
fn tok_meta_or_comment<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], Option<(String, MetaValue)>, TokExtra<'src>> + Clone
{
    // Actual metadata line
    let meta_line = tok_newline()
        .ignore_then(tok_indent())
        .ignore_then(tok_meta_key())
        .then(tok_meta_value().or_not())
        .then_ignore(tok_comment().or_not())
        .map(|(key, value)| Some((key.to_string(), value.unwrap_or(MetaValue::None))));

    // Comment-only line (skip it)
    let comment_line = tok_newline()
        .ignore_then(tok_indent())
        .ignore_then(tok_comment())
        .to(None);

    choice((meta_line, comment_line))
}

/// Parse metadata lines, filtering out comment-only lines.
fn tok_meta_lines<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], Vec<(String, MetaValue)>, TokExtra<'src>> + Clone
{
    tok_meta_or_comment()
        .repeated()
        .collect::<Vec<_>>()
        .map(|items| items.into_iter().flatten().collect())
}

/// Parse posting or metadata inside a transaction.
fn tok_posting_or_meta<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], Option<PostingOrMeta>, TokExtra<'src>> + Clone {
    let meta_entry = tok_newline()
        .ignore_then(tok_indent())
        .ignore_then(tok_meta_key())
        .then(tok_meta_value().or_not())
        .then_ignore(tok_comment().or_not())
        .map(|(k, v)| {
            Some(PostingOrMeta::Meta(
                k.to_string(),
                v.unwrap_or(MetaValue::None),
            ))
        });

    let tag_or_link = choice((
        tok_tag().map(|t| (Some(t.to_string()), None)),
        tok_link().map(|l| (None, Some(l.to_string()))),
    ));

    let tags_links_line = tok_newline()
        .ignore_then(tok_indent())
        .ignore_then(tag_or_link.repeated().at_least(1).collect::<Vec<_>>())
        .then_ignore(tok_comment().or_not())
        .map(|items| {
            let mut tags = Vec::new();
            let mut links = Vec::new();
            for (t, l) in items {
                if let Some(tag) = t {
                    tags.push(tag);
                }
                if let Some(link) = l {
                    links.push(link);
                }
            }
            Some(PostingOrMeta::TagsLinks(tags, links))
        });

    let posting_line = tok_newline()
        .ignore_then(tok_indent())
        .ignore_then(tok_posting_with_meta())
        .map(|p| Some(PostingOrMeta::Posting(p)));

    // Comment with indentation (within posting block)
    let comment_line = tok_newline()
        .ignore_then(tok_indent())
        .ignore_then(tok_comment())
        .to(None);

    // Comment without indentation (at column 0) - still allowed within transaction
    let unindented_comment = tok_newline().ignore_then(tok_comment()).to(None);

    choice((
        meta_entry,
        tags_links_line,
        posting_line,
        comment_line,
        unindented_comment,
    ))
}

/// Parse a transaction directive.
fn tok_transaction_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    let header_item = choice((
        tok_string().map(TxnHeaderItem::String),
        tok_tag().map(|t| TxnHeaderItem::Tag(t.to_string())),
        tok_link().map(|l| TxnHeaderItem::Link(l.to_string())),
    ));

    tok_date()
        .then(choice((tok_txn().to(None), tok_flag().map(Some))))
        .then(header_item.repeated().collect::<Vec<_>>())
        .then_ignore(tok_comment().or_not())
        .then(tok_posting_or_meta().repeated().collect::<Vec<_>>())
        .map(|(((date, flag_opt), header_items), items)| {
            let flag = flag_opt.unwrap_or('*');

            let mut strings = Vec::new();
            let mut tags = Vec::new();
            let mut links = Vec::new();

            for item in header_items {
                match item {
                    TxnHeaderItem::String(s) => strings.push(s),
                    TxnHeaderItem::Tag(t) => tags.push(t),
                    TxnHeaderItem::Link(l) => links.push(l),
                }
            }

            let (payee, narration) = match strings.len() {
                0 => (None, String::new()),
                1 => (None, strings.remove(0)),
                _ => (Some(strings.remove(0)), strings.remove(0)),
            };

            let mut txn = Transaction::new(date, narration).with_flag(flag);
            if let Some(p) = payee {
                txn = txn.with_payee(p);
            }
            for t in tags {
                txn = txn.with_tag(&t);
            }
            for l in links {
                txn = txn.with_link(&l);
            }
            for item in items.into_iter().flatten() {
                match item {
                    PostingOrMeta::Posting(p) => {
                        txn = txn.with_posting(p);
                    }
                    PostingOrMeta::Meta(k, v) => {
                        txn.meta.insert(k, v);
                    }
                    PostingOrMeta::TagsLinks(t, l) => {
                        for tag in t {
                            txn = txn.with_tag(&tag);
                        }
                        for link in l {
                            txn = txn.with_link(&link);
                        }
                    }
                }
            }
            (date, Directive::Transaction(txn))
        })
}

/// Parse a balance directive.
/// Format: DATE balance ACCOUNT NUMBER [~ TOLERANCE] CURRENCY [COST]
fn tok_balance_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    // Amount with optional tolerance: EXPR [~ TOLERANCE] CURRENCY
    // e.g., "200 USD", "200 ~ 0.002 USD", "(1 + 5) / 2.1 USD"
    let tolerance = tok_tilde().ignore_then(tok_expr());

    let amount_with_tolerance = tok_expr()
        .then(tolerance.or_not())
        .then(tok_currency())
        .map(|((num, tol), curr)| (Amount::new(num, curr), tol));

    tok_date()
        .then_ignore(tok_balance())
        .then(tok_account())
        .then(amount_with_tolerance)
        .then(tok_cost_spec().or_not()) // Optional cost spec for balance with cost
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(
            |((((date, account), (amount, tolerance)), _cost), meta)| {
                let mut bal = Balance::new(date, account, amount);
                if let Some(t) = tolerance {
                    bal = bal.with_tolerance(t);
                }
                for (k, v) in meta {
                    bal.meta.insert(k, v);
                }
                (date, Directive::Balance(bal))
            },
        )
}

/// Parse an open directive.
fn tok_open_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    tok_date()
        .then_ignore(tok_open())
        .then(tok_account())
        .then(tok_currency().separated_by(tok_comma()).collect::<Vec<_>>())
        .then(tok_string().or_not())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|((((date, account), currencies), booking), meta)| {
            let currencies: Vec<InternedStr> = currencies.into_iter().map(Into::into).collect();
            let mut open = Open::new(date, account).with_currencies(currencies);
            if let Some(b) = booking {
                open = open.with_booking(&b);
            }
            for (k, v) in meta {
                open.meta.insert(k, v);
            }
            (date, Directive::Open(open))
        })
}

/// Parse a close directive.
fn tok_close_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    tok_date()
        .then_ignore(tok_close())
        .then(tok_account())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|((date, account), meta)| {
            let mut close = Close::new(date, account);
            for (k, v) in meta {
                close.meta.insert(k, v);
            }
            (date, Directive::Close(close))
        })
}

/// Parse a commodity directive.
fn tok_commodity_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    tok_date()
        .then_ignore(tok_commodity())
        .then(tok_currency())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|((date, currency), meta)| {
            let mut commodity = Commodity::new(date, currency);
            for (k, v) in meta {
                commodity.meta.insert(k, v);
            }
            (date, Directive::Commodity(commodity))
        })
}

/// Parse a pad directive.
fn tok_pad_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    tok_date()
        .then_ignore(tok_pad())
        .then(tok_account())
        .then(tok_account())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|(((date, account), source), meta)| {
            let mut pad = Pad::new(date, account, source);
            for (k, v) in meta {
                pad.meta.insert(k, v);
            }
            (date, Directive::Pad(pad))
        })
}

/// Parse an event directive.
fn tok_event_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    tok_date()
        .then_ignore(tok_event())
        .then(tok_string())
        .then(tok_string())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|(((date, name), value), meta)| {
            let mut event = Event::new(date, &name, &value);
            for (k, v) in meta {
                event.meta.insert(k, v);
            }
            (date, Directive::Event(event))
        })
}

/// Parse a query directive.
fn tok_query_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    tok_date()
        .then_ignore(tok_query())
        .then(tok_string())
        .then(tok_string())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|(((date, name), query_string), meta)| {
            let mut query = Query::new(date, &name, &query_string);
            for (k, v) in meta {
                query.meta.insert(k, v);
            }
            (date, Directive::Query(query))
        })
}

/// Parse a note directive.
fn tok_note_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    tok_date()
        .then_ignore(tok_note())
        .then(tok_account())
        .then(tok_string())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|(((date, account), comment), meta)| {
            let mut note = Note::new(date, account, &comment);
            for (k, v) in meta {
                note.meta.insert(k, v);
            }
            (date, Directive::Note(note))
        })
}

/// Parse a document directive (with optional tags and links).
fn tok_document_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    // Tags and links after the path
    let tag_or_link = choice((
        tok_tag().map(|t| (Some(t.to_string()), None)),
        tok_link().map(|l| (None, Some(l.to_string()))),
    ));

    tok_date()
        .then_ignore(tok_document())
        .then(tok_account())
        .then(tok_string())
        .then(tag_or_link.repeated().collect::<Vec<_>>())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|((((date, account), path), tags_links), meta)| {
            let mut tags = Vec::new();
            let mut links = Vec::new();
            for (t, l) in tags_links {
                if let Some(tag) = t {
                    tags.push(tag);
                }
                if let Some(link) = l {
                    links.push(link);
                }
            }
            let mut document = Document::new(date, account, &path);
            document.tags = tags.into_iter().map(InternedStr::from).collect();
            document.links = links.into_iter().map(InternedStr::from).collect();
            for (k, v) in meta {
                document.meta.insert(k, v);
            }
            (date, Directive::Document(document))
        })
}

/// Parse a price directive.
fn tok_price_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    tok_date()
        .then_ignore(tok_price())
        .then(tok_currency())
        .then(tok_amount())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|(((date, currency), amount), meta)| {
            let mut price = Price::new(date, currency, amount);
            for (k, v) in meta {
                price.meta.insert(k, v);
            }
            (date, Directive::Price(price))
        })
}

/// Parse a custom directive.
fn tok_custom_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (NaiveDate, Directive), TokExtra<'src>> {
    tok_date()
        .then_ignore(tok_custom())
        .then(tok_string())
        .then(tok_meta_value().repeated().collect::<Vec<_>>())
        .then_ignore(tok_comment().or_not())
        .then(tok_meta_lines())
        .map(|(((date, name), values), meta)| {
            let mut custom = Custom::new(date, &name);
            for v in values {
                custom = custom.with_value(v);
            }
            for (k, v) in meta {
                custom.meta.insert(k, v);
            }
            (date, Directive::Custom(custom))
        })
}

/// Parse a dated directive.
fn tok_dated_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], ParsedItem, TokExtra<'src>> {
    choice((
        tok_transaction_directive(),
        tok_balance_directive(),
        tok_open_directive(),
        tok_close_directive(),
        tok_commodity_directive(),
        tok_pad_directive(),
        tok_event_directive(),
        tok_query_directive(),
        tok_note_directive(),
        tok_document_directive(),
        tok_price_directive(),
        tok_custom_directive(),
    ))
    .map(|(_, directive)| ParsedItem::Directive(directive))
}

/// Match a shebang line (e.g., #!/usr/bin/env bean-web).
fn tok_shebang<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone
{
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::Shebang(_)))
        .to(())
}

/// Match an Emacs directive (e.g., #+STARTUP: showall).
fn tok_emacs_directive<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone {
    any()
        .filter(|t: &SpannedToken<'_>| matches!(t.token, Token::EmacsDirective(_)))
        .to(())
}

/// Match an org-mode style header line (e.g., "* Options", "** Section").
/// These are lines starting with one or more `*` at the beginning of a line,
/// used for organization but ignored by beancount.
fn tok_org_header_line<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>> + Clone {
    // Match one or more Star tokens followed by any non-newline tokens until newline
    tok_star()
        .repeated()
        .at_least(1)
        .then(
            any()
                .filter(|t: &SpannedToken<'_>| !matches!(t.token, Token::Newline))
                .repeated(),
        )
        .to(())
}

/// Parse a single entry (directive, special directive, or comment).
fn tok_entry<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], ParsedItem, TokExtra<'src>> {
    choice((
        tok_option_directive(),
        tok_include_directive(),
        tok_plugin_directive(),
        tok_pushtag_directive(),
        tok_poptag_directive(),
        tok_pushmeta_directive(),
        tok_popmeta_directive(),
        tok_dated_directive(),
        tok_comment().to(ParsedItem::Comment),
        // Skip shebang, Emacs directives, and org-mode headers as comment-like entries
        tok_shebang().to(ParsedItem::Comment),
        tok_emacs_directive().to(ParsedItem::Comment),
        tok_org_header_line().to(ParsedItem::Comment),
    ))
}

/// Skip tokens until we reach a newline (for error recovery).
/// Consumes at least one token to make progress.
fn tok_skip_to_newline<'src>() -> impl Parser<'src, &'src [SpannedToken<'src>], (), TokExtra<'src>>
{
    // Must consume at least one token to make progress
    any()
        .then(
            any()
                .filter(|t: &SpannedToken<'_>| !matches!(t.token, Token::Newline))
                .repeated(),
        )
        .then(tok_newline().or_not())
        .to(())
}

/// Parse a complete file with error recovery.
fn tok_file_parser<'src>(
) -> impl Parser<'src, &'src [SpannedToken<'src>], Vec<(ParsedItem, usize, usize)>, TokExtra<'src>>
{
    // Skip leading newlines
    tok_newline().repeated().ignore_then(
        // Try to parse an entry, or skip a bad line on failure
        tok_entry()
            .map_with(|item, e| Some((item, e.span().start, e.span().end)))
            .recover_with(via_parser(
                // On error, skip to the next newline and emit None
                tok_skip_to_newline().to(None),
            ))
            .then_ignore(tok_newline().repeated())
            .repeated()
            .collect::<Vec<_>>()
            .map(|items| items.into_iter().flatten().collect()),
    )
}

// ============================================================================
// Public API
// ============================================================================

/// Parse beancount source code using token-based parser.
pub fn parse(source: &str) -> ParseResult {
    let tokens = make_tokens(source);
    let (items, errs) = tok_file_parser()
        .parse(tokens.as_slice())
        .into_output_errors();

    let items = items.unwrap_or_default();

    let mut directives = Vec::new();
    let mut options = Vec::new();
    let mut includes = Vec::new();
    let mut plugins = Vec::new();

    // Tag stack for pushtag/poptag
    let mut tag_stack: Vec<InternedStr> = Vec::new();
    // Meta stack for pushmeta/popmeta
    let mut meta_stack: Vec<(String, MetaValue)> = Vec::new();

    for (item, start_idx, end_idx) in items {
        let span = index_to_byte_span(&tokens, start_idx, end_idx);
        match item {
            ParsedItem::Directive(d) => {
                // Apply pushed tags to transactions
                let d = apply_pushed_tags(d, &tag_stack);
                // Apply pushed meta to all directives
                let d = apply_pushed_meta(d, &meta_stack);
                directives.push(Spanned::new(d, span));
            }
            ParsedItem::Option(k, v) => options.push((k, v, span)),
            ParsedItem::Include(p) => includes.push((p, span)),
            ParsedItem::Plugin(p, c) => plugins.push((p, c, span)),
            ParsedItem::Pushtag(tag) => tag_stack.push(tag.into()),
            ParsedItem::Poptag(tag) => {
                if let Some(pos) = tag_stack.iter().rposition(|t| t.as_str() == tag) {
                    tag_stack.remove(pos);
                }
            }
            ParsedItem::Pushmeta(key, value) => meta_stack.push((key, value)),
            ParsedItem::Popmeta(key) => {
                if let Some(pos) = meta_stack.iter().rposition(|(k, _)| k == &key) {
                    meta_stack.remove(pos);
                }
            }
            ParsedItem::Comment => {}
        }
    }

    let errors: Vec<ParseError> = errs
        .into_iter()
        .map(|e| {
            let start_idx = e.span().start;
            let end_idx = e.span().end;
            let span = index_to_byte_span(&tokens, start_idx, end_idx);
            let kind = if e.found().is_none() {
                ParseErrorKind::UnexpectedEof
            } else {
                // Format error manually since Rich doesn't impl Display for our token type
                let found_str = e
                    .found()
                    .map(|t| format!("{}", t.token))
                    .unwrap_or_default();
                ParseErrorKind::SyntaxError(format!("unexpected token: {found_str}"))
            };
            ParseError::new(kind, span)
        })
        .collect();

    ParseResult {
        directives,
        options,
        includes,
        plugins,
        errors,
    }
}

/// Apply pushed tags to a directive (only affects transactions).
fn apply_pushed_tags(directive: Directive, tag_stack: &[InternedStr]) -> Directive {
    if tag_stack.is_empty() {
        return directive;
    }

    match directive {
        Directive::Transaction(mut txn) => {
            for tag in tag_stack {
                if !txn.tags.contains(tag) {
                    txn.tags.push(tag.clone());
                }
            }
            Directive::Transaction(txn)
        }
        other => other,
    }
}

/// Apply pushed metadata to a directive.
fn apply_pushed_meta(directive: Directive, meta_stack: &[(String, MetaValue)]) -> Directive {
    if meta_stack.is_empty() {
        return directive;
    }

    match directive {
        Directive::Transaction(mut txn) => {
            for (key, value) in meta_stack {
                if !txn.meta.contains_key(key) {
                    txn.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Transaction(txn)
        }
        Directive::Balance(mut bal) => {
            for (key, value) in meta_stack {
                if !bal.meta.contains_key(key) {
                    bal.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Balance(bal)
        }
        Directive::Open(mut open) => {
            for (key, value) in meta_stack {
                if !open.meta.contains_key(key) {
                    open.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Open(open)
        }
        Directive::Close(mut close) => {
            for (key, value) in meta_stack {
                if !close.meta.contains_key(key) {
                    close.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Close(close)
        }
        Directive::Commodity(mut commodity) => {
            for (key, value) in meta_stack {
                if !commodity.meta.contains_key(key) {
                    commodity.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Commodity(commodity)
        }
        Directive::Pad(mut pad) => {
            for (key, value) in meta_stack {
                if !pad.meta.contains_key(key) {
                    pad.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Pad(pad)
        }
        Directive::Event(mut event) => {
            for (key, value) in meta_stack {
                if !event.meta.contains_key(key) {
                    event.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Event(event)
        }
        Directive::Query(mut query) => {
            for (key, value) in meta_stack {
                if !query.meta.contains_key(key) {
                    query.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Query(query)
        }
        Directive::Note(mut note) => {
            for (key, value) in meta_stack {
                if !note.meta.contains_key(key) {
                    note.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Note(note)
        }
        Directive::Document(mut document) => {
            for (key, value) in meta_stack {
                if !document.meta.contains_key(key) {
                    document.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Document(document)
        }
        Directive::Price(mut price) => {
            for (key, value) in meta_stack {
                if !price.meta.contains_key(key) {
                    price.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Price(price)
        }
        Directive::Custom(mut custom) => {
            for (key, value) in meta_stack {
                if !custom.meta.contains_key(key) {
                    custom.meta.insert(key.clone(), value.clone());
                }
            }
            Directive::Custom(custom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_option() {
        let result = parse(r#"option "title" "My Ledger""#);
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
        assert_eq!(result.options.len(), 1);
        assert_eq!(result.options[0].0, "title");
        assert_eq!(result.options[0].1, "My Ledger");
    }

    #[test]
    fn test_parse_open() {
        let result = parse("2024-01-15 open Assets:Bank USD");
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
        assert_eq!(result.directives.len(), 1);
        if let Directive::Open(open) = &result.directives[0].value {
            assert_eq!(open.account, "Assets:Bank");
            assert_eq!(open.currencies, vec!["USD"]);
        } else {
            panic!("Expected Open directive");
        }
    }

    #[test]
    fn test_parse_transaction() {
        let result = parse(
            r#"2024-01-15 * "Coffee Shop" "Morning coffee"
  Expenses:Food:Coffee  5.00 USD
  Assets:Cash"#,
        );
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
        assert_eq!(result.directives.len(), 1);
        if let Directive::Transaction(txn) = &result.directives[0].value {
            assert_eq!(
                txn.payee.as_ref().map(InternedStr::as_str),
                Some("Coffee Shop")
            );
            assert_eq!(txn.narration, "Morning coffee");
            assert_eq!(txn.postings.len(), 2);
        } else {
            panic!("Expected Transaction directive");
        }
    }

    #[test]
    fn test_parse_balance() {
        let result = parse("2024-01-15 balance Assets:Bank 1000.00 USD");
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
        assert_eq!(result.directives.len(), 1);
        if let Directive::Balance(bal) = &result.directives[0].value {
            assert_eq!(bal.account, "Assets:Bank");
        } else {
            panic!("Expected Balance directive");
        }
    }

    #[test]
    fn test_parse_price() {
        let result = parse("2024-01-15 price USD 0.85 EUR");
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
        assert_eq!(result.directives.len(), 1);
        if let Directive::Price(price) = &result.directives[0].value {
            assert_eq!(price.currency, "USD");
        } else {
            panic!("Expected Price directive");
        }
    }

    #[test]
    fn test_parse_complete_ledger() {
        // Test parsing a complete ledger with multiple directive types
        let source = r#"
option "title" "Test Ledger"

2024-01-01 open Assets:Bank USD
2024-01-01 open Expenses:Food USD

2024-01-15 * "Store" "Groceries"
  Expenses:Food  50.00 USD
  Assets:Bank

2024-01-15 balance Assets:Bank 1000.00 USD
"#;
        let result = parse(source);

        assert_eq!(result.directives.len(), 4, "Expected 4 directives");
        assert_eq!(result.options.len(), 1, "Expected 1 option");
        assert!(
            result.errors.is_empty(),
            "Parser errors: {:?}",
            result.errors
        );
    }
}
