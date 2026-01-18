//! Semantic tokens handler for enhanced syntax highlighting.
//!
//! Provides semantic token information for:
//! - Dates
//! - Accounts
//! - Currencies
//! - Numbers
//! - Strings (payees, narrations)
//! - Keywords (directive types)
//! - Comments
//!
//! Supports full document, range-based, and delta tokenization.

use lsp_types::{
    Range, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensDelta, SemanticTokensDeltaParams, SemanticTokensEdit,
    SemanticTokensFullDeltaResult, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensRangeParams,
    SemanticTokensRangeResult, SemanticTokensResult, SemanticTokensServerCapabilities,
};
use rustledger_core::Directive;
use rustledger_parser::ParseResult;
use std::sync::atomic::{AtomicU64, Ordering};

use super::utils::byte_offset_to_position;

/// Token types we support.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,  // 0: directive keywords (open, close, etc.)
    SemanticTokenType::NUMBER,   // 1: amounts
    SemanticTokenType::STRING,   // 2: payees, narrations
    SemanticTokenType::VARIABLE, // 3: accounts
    SemanticTokenType::TYPE,     // 4: currencies
    SemanticTokenType::COMMENT,  // 5: comments
    SemanticTokenType::OPERATOR, // 6: flags (*, !)
    SemanticTokenType::MACRO,    // 7: dates
];

/// Token modifiers we support.
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DEFINITION, // 0: where something is defined
    SemanticTokenModifier::DEPRECATED, // 1: closed accounts
    SemanticTokenModifier::READONLY,   // 2: balance assertions
];

/// Get the semantic tokens legend for capability registration.
pub fn get_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

/// Counter for generating unique result IDs.
static RESULT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a new unique result ID.
fn generate_result_id() -> String {
    RESULT_ID_COUNTER.fetch_add(1, Ordering::SeqCst).to_string()
}

/// Get the semantic tokens server capabilities.
pub fn get_capabilities() -> SemanticTokensServerCapabilities {
    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
        legend: get_legend(),
        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }), // Enable delta support
        range: Some(true), // Enable range-based tokenization
        work_done_progress_options: Default::default(),
    })
}

/// Token type indices.
mod token_type {
    pub const KEYWORD: u32 = 0;
    pub const NUMBER: u32 = 1;
    pub const STRING: u32 = 2;
    pub const VARIABLE: u32 = 3; // accounts
    pub const TYPE: u32 = 4; // currencies
    #[allow(dead_code)] // Reserved for future use when we parse comments
    pub const COMMENT: u32 = 5;
    pub const OPERATOR: u32 = 6; // flags
    pub const MACRO: u32 = 7; // dates
}

/// Token modifier bits.
mod token_modifier {
    pub const DEFINITION: u32 = 1 << 0;
    pub const DEPRECATED: u32 = 1 << 1;
    #[allow(dead_code)]
    pub const READONLY: u32 = 1 << 2;
}

/// Handle a semantic tokens request.
pub fn handle_semantic_tokens(
    _params: &SemanticTokensParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<SemanticTokensResult> {
    let mut tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    // Collect all tokens from directives
    let mut raw_tokens: Vec<RawToken> = Vec::new();

    for spanned in &parse_result.directives {
        collect_directive_tokens(&spanned.value, spanned.span.start, source, &mut raw_tokens);
    }

    // Sort tokens by position
    raw_tokens.sort_by_key(|t| (t.line, t.start));

    // Convert to delta-encoded semantic tokens
    for raw in raw_tokens {
        let delta_line = raw.line - prev_line;
        let delta_start = if delta_line == 0 {
            raw.start - prev_start
        } else {
            raw.start
        };

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: raw.length,
            token_type: raw.token_type,
            token_modifiers_bitset: raw.modifiers,
        });

        prev_line = raw.line;
        prev_start = raw.start;
    }

    if tokens.is_empty() {
        None
    } else {
        Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: Some(generate_result_id()),
            data: tokens,
        }))
    }
}

/// Handle a semantic tokens delta request.
/// Returns only the changed tokens since the previous result.
///
/// Note: For simplicity, this implementation always returns full tokens
/// when there are changes, using the edit mechanism. A more sophisticated
/// implementation could compute actual diffs for better performance.
pub fn handle_semantic_tokens_delta(
    params: &SemanticTokensDeltaParams,
    source: &str,
    parse_result: &ParseResult,
    previous_tokens: Option<&[SemanticToken]>,
) -> Option<SemanticTokensFullDeltaResult> {
    // Compute current tokens
    let mut current_tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    let mut raw_tokens: Vec<RawToken> = Vec::new();
    for spanned in &parse_result.directives {
        collect_directive_tokens(&spanned.value, spanned.span.start, source, &mut raw_tokens);
    }
    raw_tokens.sort_by_key(|t| (t.line, t.start));

    for raw in raw_tokens {
        let delta_line = raw.line - prev_line;
        let delta_start = if delta_line == 0 {
            raw.start - prev_start
        } else {
            raw.start
        };

        current_tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: raw.length,
            token_type: raw.token_type,
            token_modifiers_bitset: raw.modifiers,
        });

        prev_line = raw.line;
        prev_start = raw.start;
    }

    // If we have previous tokens and they match, return empty delta
    if let Some(prev) = previous_tokens {
        if tokens_equal(prev, &current_tokens) {
            return Some(SemanticTokensFullDeltaResult::TokensDelta(
                SemanticTokensDelta {
                    result_id: Some(generate_result_id()),
                    edits: vec![], // No changes
                },
            ));
        }
    }

    // Tokens changed - return full replacement as a single edit
    // This replaces all tokens from index 0
    let new_result_id = generate_result_id();
    let _ = params; // Used for previous_result_id validation in a more complete impl

    if current_tokens.is_empty() && previous_tokens.map(|t| t.is_empty()).unwrap_or(true) {
        return None;
    }

    let prev_len = previous_tokens.map(|t| t.len()).unwrap_or(0);

    Some(SemanticTokensFullDeltaResult::TokensDelta(
        SemanticTokensDelta {
            result_id: Some(new_result_id),
            edits: vec![SemanticTokensEdit {
                start: 0,
                delete_count: prev_len as u32,
                data: Some(current_tokens),
            }],
        },
    ))
}

/// Check if two token arrays are equal.
fn tokens_equal(a: &[SemanticToken], b: &[SemanticToken]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        x.delta_line == y.delta_line
            && x.delta_start == y.delta_start
            && x.length == y.length
            && x.token_type == y.token_type
            && x.token_modifiers_bitset == y.token_modifiers_bitset
    })
}

/// Handle a semantic tokens range request.
/// Only tokenizes directives within the requested range for better performance.
pub fn handle_semantic_tokens_range(
    params: &SemanticTokensRangeParams,
    source: &str,
    parse_result: &ParseResult,
) -> Option<SemanticTokensRangeResult> {
    let range = params.range;
    let mut tokens = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    // Collect tokens only from directives within the range
    let mut raw_tokens: Vec<RawToken> = Vec::new();

    for spanned in &parse_result.directives {
        let (dir_line, _) = byte_offset_to_position(source, spanned.span.start);

        // Skip directives before the range
        if dir_line > range.end.line {
            continue;
        }

        // Skip directives after the range (estimate end based on directive type)
        let estimated_end_line = estimate_directive_end_line(dir_line, &spanned.value);
        if estimated_end_line < range.start.line {
            continue;
        }

        collect_directive_tokens(&spanned.value, spanned.span.start, source, &mut raw_tokens);
    }

    // Sort tokens by position
    raw_tokens.sort_by_key(|t| (t.line, t.start));

    // Filter tokens within range and convert to delta-encoded
    for raw in raw_tokens {
        // Skip tokens outside the requested range
        if !is_token_in_range(&raw, &range) {
            continue;
        }

        let delta_line = raw.line - prev_line;
        let delta_start = if delta_line == 0 {
            raw.start - prev_start
        } else {
            raw.start
        };

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length: raw.length,
            token_type: raw.token_type,
            token_modifiers_bitset: raw.modifiers,
        });

        prev_line = raw.line;
        prev_start = raw.start;
    }

    if tokens.is_empty() {
        None
    } else {
        Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        }))
    }
}

/// Estimate the end line of a directive for range filtering.
fn estimate_directive_end_line(start_line: u32, directive: &Directive) -> u32 {
    match directive {
        Directive::Transaction(txn) => {
            // Transaction spans header + postings
            start_line + 1 + txn.postings.len() as u32
        }
        _ => {
            // Most directives are single line
            start_line
        }
    }
}

/// Check if a token is within the requested range.
fn is_token_in_range(token: &RawToken, range: &Range) -> bool {
    // Token line must be within range
    if token.line < range.start.line || token.line > range.end.line {
        return false;
    }

    // If on start line, token must start at or after range start
    if token.line == range.start.line && token.start < range.start.character {
        return false;
    }

    // If on end line, token must end at or before range end
    if token.line == range.end.line && token.start + token.length > range.end.character {
        return false;
    }

    true
}

/// A raw token before delta encoding.
struct RawToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    modifiers: u32,
}

/// Collect tokens from a directive.
fn collect_directive_tokens(
    directive: &Directive,
    start_offset: usize,
    source: &str,
    tokens: &mut Vec<RawToken>,
) {
    let (line, col) = byte_offset_to_position(source, start_offset);

    match directive {
        Directive::Transaction(txn) => {
            // Date token
            tokens.push(RawToken {
                line,
                start: col,
                length: 10, // YYYY-MM-DD
                token_type: token_type::MACRO,
                modifiers: 0,
            });

            // Flag token (after date + space)
            let flag_col = col + 11;
            tokens.push(RawToken {
                line,
                start: flag_col,
                length: 1,
                token_type: token_type::OPERATOR,
                modifiers: 0,
            });

            // Payee if present (estimate position)
            if let Some(ref payee) = txn.payee {
                let payee_len = payee.len() as u32 + 2; // include quotes
                tokens.push(RawToken {
                    line,
                    start: flag_col + 2,
                    length: payee_len,
                    token_type: token_type::STRING,
                    modifiers: 0,
                });
            }

            // Postings
            for (i, posting) in txn.postings.iter().enumerate() {
                let posting_line = line + 1 + i as u32;

                // Account
                let account_str = posting.account.to_string();
                tokens.push(RawToken {
                    line: posting_line,
                    start: 2, // indentation
                    length: account_str.len() as u32,
                    token_type: token_type::VARIABLE,
                    modifiers: 0,
                });

                // Amount if present
                if let Some(ref units) = posting.units {
                    if let Some(num) = units.number() {
                        let num_str = num.to_string();
                        let num_start = 2 + account_str.len() as u32 + 2;
                        tokens.push(RawToken {
                            line: posting_line,
                            start: num_start,
                            length: num_str.len() as u32,
                            token_type: token_type::NUMBER,
                            modifiers: 0,
                        });

                        // Currency
                        if let Some(curr) = units.currency() {
                            let curr_str = curr.to_string();
                            tokens.push(RawToken {
                                line: posting_line,
                                start: num_start + num_str.len() as u32 + 1,
                                length: curr_str.len() as u32,
                                token_type: token_type::TYPE,
                                modifiers: 0,
                            });
                        }
                    }
                }
            }
        }

        Directive::Open(open) => {
            // Date
            tokens.push(RawToken {
                line,
                start: col,
                length: 10,
                token_type: token_type::MACRO,
                modifiers: 0,
            });

            // "open" keyword
            tokens.push(RawToken {
                line,
                start: col + 11,
                length: 4,
                token_type: token_type::KEYWORD,
                modifiers: 0,
            });

            // Account (definition)
            let account_str = open.account.to_string();
            tokens.push(RawToken {
                line,
                start: col + 16,
                length: account_str.len() as u32,
                token_type: token_type::VARIABLE,
                modifiers: token_modifier::DEFINITION,
            });

            // Currencies
            let mut curr_start = col + 17 + account_str.len() as u32;
            for curr in &open.currencies {
                let curr_str = curr.to_string();
                tokens.push(RawToken {
                    line,
                    start: curr_start,
                    length: curr_str.len() as u32,
                    token_type: token_type::TYPE,
                    modifiers: 0,
                });
                curr_start += curr_str.len() as u32 + 1;
            }
        }

        Directive::Close(close) => {
            // Date
            tokens.push(RawToken {
                line,
                start: col,
                length: 10,
                token_type: token_type::MACRO,
                modifiers: 0,
            });

            // "close" keyword
            tokens.push(RawToken {
                line,
                start: col + 11,
                length: 5,
                token_type: token_type::KEYWORD,
                modifiers: 0,
            });

            // Account (deprecated)
            let account_str = close.account.to_string();
            tokens.push(RawToken {
                line,
                start: col + 17,
                length: account_str.len() as u32,
                token_type: token_type::VARIABLE,
                modifiers: token_modifier::DEPRECATED,
            });
        }

        Directive::Balance(bal) => {
            // Date
            tokens.push(RawToken {
                line,
                start: col,
                length: 10,
                token_type: token_type::MACRO,
                modifiers: 0,
            });

            // "balance" keyword
            tokens.push(RawToken {
                line,
                start: col + 11,
                length: 7,
                token_type: token_type::KEYWORD,
                modifiers: 0,
            });

            // Account
            let account_str = bal.account.to_string();
            tokens.push(RawToken {
                line,
                start: col + 19,
                length: account_str.len() as u32,
                token_type: token_type::VARIABLE,
                modifiers: 0,
            });

            // Amount
            let num_str = bal.amount.number.to_string();
            let num_start = col + 20 + account_str.len() as u32;
            tokens.push(RawToken {
                line,
                start: num_start,
                length: num_str.len() as u32,
                token_type: token_type::NUMBER,
                modifiers: 0,
            });

            // Currency
            let curr_str = bal.amount.currency.to_string();
            tokens.push(RawToken {
                line,
                start: num_start + num_str.len() as u32 + 1,
                length: curr_str.len() as u32,
                token_type: token_type::TYPE,
                modifiers: 0,
            });
        }

        Directive::Commodity(comm) => {
            // Date
            tokens.push(RawToken {
                line,
                start: col,
                length: 10,
                token_type: token_type::MACRO,
                modifiers: 0,
            });

            // "commodity" keyword
            tokens.push(RawToken {
                line,
                start: col + 11,
                length: 9,
                token_type: token_type::KEYWORD,
                modifiers: 0,
            });

            // Currency (definition)
            let curr_str = comm.currency.to_string();
            tokens.push(RawToken {
                line,
                start: col + 21,
                length: curr_str.len() as u32,
                token_type: token_type::TYPE,
                modifiers: token_modifier::DEFINITION,
            });
        }

        Directive::Price(price) => {
            // Date
            tokens.push(RawToken {
                line,
                start: col,
                length: 10,
                token_type: token_type::MACRO,
                modifiers: 0,
            });

            // "price" keyword
            tokens.push(RawToken {
                line,
                start: col + 11,
                length: 5,
                token_type: token_type::KEYWORD,
                modifiers: 0,
            });

            // Currency
            let curr_str = price.currency.to_string();
            tokens.push(RawToken {
                line,
                start: col + 17,
                length: curr_str.len() as u32,
                token_type: token_type::TYPE,
                modifiers: 0,
            });

            // Amount
            let num_str = price.amount.number.to_string();
            let num_start = col + 18 + curr_str.len() as u32;
            tokens.push(RawToken {
                line,
                start: num_start,
                length: num_str.len() as u32,
                token_type: token_type::NUMBER,
                modifiers: 0,
            });

            // Target currency
            let target_curr = price.amount.currency.to_string();
            tokens.push(RawToken {
                line,
                start: num_start + num_str.len() as u32 + 1,
                length: target_curr.len() as u32,
                token_type: token_type::TYPE,
                modifiers: 0,
            });
        }

        // For other directives, just highlight the date and keyword
        _ => {
            // Date
            tokens.push(RawToken {
                line,
                start: col,
                length: 10,
                token_type: token_type::MACRO,
                modifiers: 0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustledger_parser::parse;

    #[test]
    fn test_semantic_tokens_basic() {
        let source = "2024-01-01 open Assets:Bank USD\n";
        let result = parse(source);
        let params = SemanticTokensParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response = handle_semantic_tokens(&params, source, &result);
        assert!(response.is_some());

        if let Some(SemanticTokensResult::Tokens(tokens)) = response {
            // Should have tokens for: date, keyword, account, currency
            assert!(!tokens.data.is_empty());
        }
    }

    #[test]
    fn test_semantic_tokens_range() {
        let source = r#"2024-01-01 open Assets:Bank USD
2024-01-15 * "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
2024-01-20 close Assets:OldAccount
"#;
        let result = parse(source);

        // Request tokens only for lines 1-3 (the transaction)
        let params = SemanticTokensRangeParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            range: Range {
                start: lsp_types::Position::new(1, 0),
                end: lsp_types::Position::new(3, 100),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let response = handle_semantic_tokens_range(&params, source, &result);
        assert!(response.is_some());

        if let Some(SemanticTokensRangeResult::Tokens(tokens)) = response {
            // Should have tokens, but fewer than full document
            assert!(!tokens.data.is_empty());
        }
    }

    #[test]
    fn test_is_token_in_range() {
        let token = RawToken {
            line: 5,
            start: 10,
            length: 5,
            token_type: 0,
            modifiers: 0,
        };

        // Token fully in range
        let range = Range {
            start: lsp_types::Position::new(0, 0),
            end: lsp_types::Position::new(10, 100),
        };
        assert!(is_token_in_range(&token, &range));

        // Token before range
        let range = Range {
            start: lsp_types::Position::new(6, 0),
            end: lsp_types::Position::new(10, 100),
        };
        assert!(!is_token_in_range(&token, &range));

        // Token after range
        let range = Range {
            start: lsp_types::Position::new(0, 0),
            end: lsp_types::Position::new(4, 100),
        };
        assert!(!is_token_in_range(&token, &range));
    }

    #[test]
    fn test_semantic_tokens_delta_no_change() {
        let source = "2024-01-01 open Assets:Bank USD\n";
        let result = parse(source);

        // Get initial tokens
        let params = SemanticTokensParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let initial = handle_semantic_tokens(&params, source, &result);
        let initial_tokens = match initial {
            Some(SemanticTokensResult::Tokens(t)) => t.data,
            _ => panic!("Expected tokens"),
        };

        // Request delta with same source
        let delta_params = SemanticTokensDeltaParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            previous_result_id: "0".to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let delta =
            handle_semantic_tokens_delta(&delta_params, source, &result, Some(&initial_tokens));
        assert!(delta.is_some());

        // Should return empty edits since nothing changed
        if let Some(SemanticTokensFullDeltaResult::TokensDelta(d)) = delta {
            assert!(d.edits.is_empty());
        } else {
            panic!("Expected delta result");
        }
    }

    #[test]
    fn test_semantic_tokens_delta_with_change() {
        // Use significantly different sources to ensure different tokens
        let source1 = "2024-01-01 open Assets:Bank USD\n";
        let source2 = r#"2024-01-01 open Assets:Bank USD
2024-01-15 * "Coffee"
  Assets:Bank  -5.00 USD
  Expenses:Food
"#;

        let result1 = parse(source1);
        let result2 = parse(source2);

        // Get initial tokens
        let params = SemanticTokensParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let initial = handle_semantic_tokens(&params, source1, &result1);
        let initial_tokens = match initial {
            Some(SemanticTokensResult::Tokens(t)) => t.data,
            _ => panic!("Expected tokens"),
        };

        // Request delta with changed source
        let delta_params = SemanticTokensDeltaParams {
            text_document: lsp_types::TextDocumentIdentifier {
                uri: "file:///test.beancount".parse().unwrap(),
            },
            previous_result_id: "0".to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let delta =
            handle_semantic_tokens_delta(&delta_params, source2, &result2, Some(&initial_tokens));
        assert!(delta.is_some());

        // Should return edits since source changed significantly
        if let Some(SemanticTokensFullDeltaResult::TokensDelta(d)) = delta {
            assert!(
                !d.edits.is_empty(),
                "Expected non-empty edits for changed source"
            );
            // The edit should contain the new tokens
            assert!(d.edits[0].data.is_some());
            // New source has more directives, so should have more tokens
            let new_tokens = d.edits[0].data.as_ref().unwrap();
            assert!(new_tokens.len() > initial_tokens.len());
        } else {
            panic!("Expected delta result");
        }
    }

    #[test]
    fn test_tokens_equal() {
        let tokens1 = vec![SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 10,
            token_type: 0,
            token_modifiers_bitset: 0,
        }];
        let tokens2 = tokens1.clone();
        let tokens3 = vec![SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 11, // Different length
            token_type: 0,
            token_modifiers_bitset: 0,
        }];

        assert!(tokens_equal(&tokens1, &tokens2));
        assert!(!tokens_equal(&tokens1, &tokens3));
        assert!(!tokens_equal(&tokens1, &[]));
    }
}
