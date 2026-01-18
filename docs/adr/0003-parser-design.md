# ADR-0003: Parser Design

## Status

Accepted (Updated January 2026)

## Context

The Beancount language has a relatively simple grammar but with some complexities:

- Date-prefixed directives
- Indentation-sensitive posting syntax
- Metadata can appear on many directive types
- String literals with escapes
- Multiple number formats (with comma grouping)

Options considered:

1. **Parser generator** (pest, lalrpop): Generate parser from grammar
2. **Parser combinator** (nom, winnow, chumsky): Compose small parsers
3. **Hand-written recursive descent**: Manual implementation

## Decision

Use **Logos for lexing** and **Chumsky for parsing** (parser combinators).

### Lexer (Logos)

The lexer (`logos_lexer.rs`) uses Logos, a SIMD-accelerated lexer generator:

- Declarative token definitions via derive macros
- ~54x faster than hand-written character iteration
- Produces `Vec<SpannedToken>` with byte offset spans

Tokens include:
- Keywords (open, close, balance, etc.)
- Dates, numbers, strings, accounts, currencies
- Operators and punctuation

### Parser (Chumsky)

The parser (`token_parser.rs`) uses Chumsky parser combinators:

- Composable parsers for each directive type
- Built-in error recovery mechanisms
- Rich error types with expected/found tokens
- Span tracking propagated automatically

Architecture:
```text
Source (&str) → Logos tokenize() → Vec<SpannedToken> → Chumsky parser → Directives
```

## Consequences

### Positive

- Logos provides excellent lexer performance (SIMD-accelerated)
- Chumsky offers expressive, composable parser combinators
- Good error recovery built into the framework
- Type-safe parser composition catches errors at compile time
- No external grammar DSL files to maintain

### Negative

- Chumsky has a learning curve for complex combinators
- Compile times slightly longer due to heavy generics
- Debug output can be verbose

### Neutral

- Parser is ~2000 lines, manageable for the grammar size
- Error messages require tuning for user-friendliness

## Notes

The parser is organized into sections:

1. Token input types and helpers
2. Primitive parsers (date, number, string, account)
3. Directive parsers (transaction, balance, open, etc.)
4. Expression parsers (amount, cost, metadata, postings)
5. Top-level file parser with error recovery

## History

- **Original decision**: Hand-written recursive descent parser
- **January 2026**: Migrated to Logos + Chumsky for better performance and maintainability
