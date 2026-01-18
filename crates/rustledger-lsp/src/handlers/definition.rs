//! Go-to-definition handler.
//!
//! Provides navigation to symbol definitions:
//! - Account → Open directive
//! - Currency → Commodity directive

use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Position, Range, Uri};
use rustledger_core::Directive;
use rustledger_parser::ParseResult;

use super::utils::{
    byte_offset_to_position, get_word_at_source_position, is_account_type, is_currency_like_simple,
};

/// Handle a go-to-definition request.
pub fn handle_goto_definition(
    params: &GotoDefinitionParams,
    source: &str,
    parse_result: &ParseResult,
    uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let position = params.text_document_position_params.position;

    // Get the word at the cursor position
    let word = get_word_at_source_position(source, position)?;

    tracing::debug!("Go-to-definition for word: {:?}", word);

    // Check if it's an account name
    if word.contains(':') || is_account_type(&word) {
        if let Some(location) = find_account_definition(&word, parse_result, source, uri) {
            return Some(GotoDefinitionResponse::Scalar(location));
        }
    }

    // Check if it's a currency
    if is_currency_like_simple(&word) {
        if let Some(location) = find_currency_definition(&word, parse_result, source, uri) {
            return Some(GotoDefinitionResponse::Scalar(location));
        }
    }

    None
}

/// Find the definition of an account (the Open directive).
fn find_account_definition(
    account: &str,
    parse_result: &ParseResult,
    source: &str,
    uri: &Uri,
) -> Option<Location> {
    for spanned_directive in &parse_result.directives {
        if let Directive::Open(open) = &spanned_directive.value {
            let open_account = open.account.to_string();
            // Match exact account or account prefix
            if open_account == account || account.starts_with(&format!("{}:", open_account)) {
                let (start_line, start_col) =
                    byte_offset_to_position(source, spanned_directive.span.start);
                let (end_line, end_col) =
                    byte_offset_to_position(source, spanned_directive.span.end);

                return Some(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position::new(start_line, start_col),
                        end: Position::new(end_line, end_col),
                    },
                });
            }
        }
    }
    None
}

/// Find the definition of a currency (the Commodity directive).
fn find_currency_definition(
    currency: &str,
    parse_result: &ParseResult,
    source: &str,
    uri: &Uri,
) -> Option<Location> {
    for spanned_directive in &parse_result.directives {
        if let Directive::Commodity(comm) = &spanned_directive.value {
            if comm.currency.as_ref() == currency {
                let (start_line, start_col) =
                    byte_offset_to_position(source, spanned_directive.span.start);
                let (end_line, end_col) =
                    byte_offset_to_position(source, spanned_directive.span.end);

                return Some(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position::new(start_line, start_col),
                        end: Position::new(end_line, end_col),
                    },
                });
            }
        }
    }
    None
}
