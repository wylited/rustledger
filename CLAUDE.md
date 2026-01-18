# Claude Code Context

This document provides context for Claude Code when reviewing pull requests and assisting with development.

## Project Overview

rustledger is a pure Rust implementation of Beancount, the double-entry bookkeeping language. It provides a 10-30x faster alternative to Python beancount with full syntax compatibility.

## Architecture

The project is a Cargo workspace with 9 crates:

| Crate | Purpose |
|-------|---------|
| `rustledger-core` | Core types (Amount, Position, Inventory, Directives) |
| `rustledger-parser` | Lexer and parser with error recovery |
| `rustledger-loader` | File loading, includes, options |
| `rustledger-booking` | Interpolation and booking engine (7 methods) |
| `rustledger-validate` | Validation with 27 error codes |
| `rustledger-query` | BQL query engine |
| `rustledger-plugin` | Native and WASM plugin system (20 plugins) |
| `rustledger` | CLI tools (rledger-check, rledger-query, etc.) |
| `rustledger-wasm` | WebAssembly library target |

## Code Standards

### Rust Idioms

- Use `Result<T, E>` for fallible operations, not panics
- Prefer `?` operator over `.unwrap()` in production code
- Use `thiserror` for error types, `anyhow` in CLI/tests
- Prefer iterators over explicit loops where idiomatic
- Use `#[must_use]` on functions returning important values

### Performance

- Avoid unnecessary allocations (prefer `&str` over `String` when possible)
- Use `Cow<'a, str>` for potentially-owned strings
- Prefer `SmallVec` for small, stack-allocated collections
- Profile before optimizing - correctness first

### Testing

- Unit tests in `#[cfg(test)]` modules within source files
- Integration tests in `crates/*/tests/` directories
- Use `insta` for snapshot testing of parser output
- Use `proptest` for property-based testing
- All public APIs must have tests

### Documentation

- All public items must have doc comments
- Include examples in doc comments where helpful
- Use `# Errors` section to document error conditions
- Use `# Panics` section if function can panic

## Pull Request Review Policy

### Review Checklist

When reviewing PRs, check each of these areas:

1. **Correctness**: Does the code do what it claims?
2. **Beancount Compatibility**: Does it match Python beancount behavior?
3. **Error Handling**: Are errors handled gracefully with good messages?
4. **Tests**: Are there sufficient tests for new functionality?
5. **Performance**: Any obvious performance issues?
6. **Security**: Any potential security concerns (especially in parser/loader)?
7. **Documentation**: Are public APIs documented?
8. **Style**: Does it follow project conventions?

### Review Standards by PR Type

| PR Type | Focus Areas | Approval Threshold |
|---------|-------------|-------------------|
| Bug fix | Correctness, regression tests, no side effects | 1 approval |
| Feature | All checklist items, especially tests and docs | 1 approval |
| Parser changes | Beancount compatibility, fuzz testing, error messages | 1 approval + extra scrutiny |
| Breaking change | Migration path, documentation, all areas | 2 approvals |
| Security fix | Vulnerability addressed, no new issues introduced | 1 approval, expedited |

### Review Process

1. **Read the PR description** - Understand the intent
2. **Check CI status** - All checks should pass
3. **Review file changes** - Focus on logic, not just style
4. **Run locally if needed** - For complex changes
5. **Leave constructive feedback** - Suggest improvements, explain concerns
6. **Approve or request changes** - Be clear about blockers vs suggestions

### Common Review Comments

- "Add a test for this edge case"
- "This could panic on empty input - use `get()` instead of indexing"
- "Consider using `&str` instead of `String` here"
- "Does this match Python beancount behavior?"
- "This allocation could be avoided with..."

### Auto-merge Rules

PRs can auto-merge after CI passes if:
- Single approval obtained
- No "request changes" reviews pending
- PR is not marked as draft
- No merge conflicts

## Security Considerations

- **Parser**: Must handle malformed input gracefully (no panics)
- **Loader**: Must prevent path traversal in `include` directives
- **WASM**: Must be sandboxed, no file system access
- **Dependencies**: Check for known vulnerabilities with `cargo deny`

## Common Patterns

### Adding a new plugin

1. Create struct implementing `NativePlugin` trait in `rustledger-plugin/src/native/`
1. Register in `NativePluginRegistry::new()`
1. Add tests in `tests/native_plugins_test.rs`

### Adding a BQL function

1. Add case to `evaluate_function()` in `rustledger-query/src/executor.rs`
1. Add completion in `rustledger-query/src/completions.rs`
1. Add tests and documentation

### Adding a validation error

1. Add variant to `ValidationError` enum in `rustledger-validate/src/lib.rs`
1. Implement detection in `validate_*` function
1. Add tests covering the error case

## Build Commands

```bash
cargo check --all-features --all-targets  # Quick check
cargo test --all-features                  # Run all tests
cargo clippy --all-features -- -D warnings # Lint
cargo fmt --all -- --check                 # Format check
cargo deny check                           # Security audit
```
