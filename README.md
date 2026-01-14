# rustledger

A pure Rust implementation of [Beancount](https://beancount.github.io/), the double-entry bookkeeping language.

[![CI](https://github.com/rustledger/rustledger/actions/workflows/ci.yml/badge.svg)](https://github.com/rustledger/rustledger/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/rustledger.svg)](https://crates.io/crates/rustledger)
[![Documentation](https://docs.rs/rustledger/badge.svg)](https://docs.rs/rustledger)
[![codecov](https://codecov.io/gh/rustledger/rustledger/graph/badge.svg)](https://codecov.io/gh/rustledger/rustledger)
[![License](https://img.shields.io/crates/l/rustledger.svg)](LICENSE)

[![GitHub Release](https://img.shields.io/github/v/release/rustledger/rustledger?label=release)](https://github.com/rustledger/rustledger/releases)
[![npm](https://img.shields.io/npm/v/@rustledger/wasm?label=npm%20wasm)](https://www.npmjs.com/package/@rustledger/wasm)
[![AUR](https://img.shields.io/aur/version/rustledger?logo=arch-linux&label=AUR)](https://aur.archlinux.org/packages/rustledger)
[![Copr](https://copr.fedorainfracloud.org/coprs/robcohen/rustledger/package/rustledger/status_image/last_build.png)](https://copr.fedorainfracloud.org/coprs/robcohen/rustledger/)
[![Packaging status](https://repology.org/badge/tiny-repos/rustledger.svg)](https://repology.org/project/rustledger/versions)

## Why rustledger?

- **10x faster** than Python beancount - parse and validate large ledgers in milliseconds
- **Pure Rust** - No Python dependencies, single binary, compiles to native and WebAssembly
- **Drop-in replacement** - Compatible `bean-*` CLI commands for easy migration
- **Formally verified** - Core algorithms verified with 19 TLA+ specifications
- **Full compatibility** - Parses any valid beancount file

## Quick Start

```bash
# Install
cargo install rustledger

# Validate your ledger
rledger-check ledger.beancount

# Query your data
rledger-query ledger.beancount "SELECT account, SUM(position) GROUP BY account"
```

Example output:
```
$ rledger-check example.beancount
Loaded 1,247 directives in 12ms
✓ No errors found

$ rledger-query example.beancount "BALANCES WHERE account ~ 'Assets:'"
account                    balance
-------------------------  ----------------
Assets:Bank:Checking       2,450.00 USD
Assets:Bank:Savings       15,000.00 USD
Assets:Investments         5,230.50 USD
```

## Installation

### Quick Install (Linux/macOS)

```bash
curl -sSfL rustledger.github.io/i | sh
```

### Homebrew (macOS/Linux)

```bash
brew install rustledger/rustledger/rustledger
```

### Scoop (Windows)

```powershell
scoop bucket add rustledger https://github.com/rustledger/scoop-rustledger
scoop install rustledger
```

### Cargo

```bash
# Pre-built binary (fast)
cargo binstall rustledger

# Build from source
cargo install rustledger
```

### Nix

```bash
# Run directly
nix run github:rustledger/rustledger -- rledger-check ledger.beancount

# Install to profile
nix profile install github:rustledger/rustledger
```

### Arch Linux (AUR)

```bash
# Pre-built binary (recommended)
yay -S rustledger-bin

# Or build from source
yay -S rustledger
```

### Docker

```bash
# Validate a ledger file
docker run --rm -v "$PWD:/data" ghcr.io/rustledger/rustledger /data/ledger.beancount

# Run queries
docker run --rm -v "$PWD:/data" --entrypoint rledger-query ghcr.io/rustledger/rustledger \
  /data/ledger.beancount "SELECT account, SUM(position) GROUP BY account"

# Use a specific version
docker run --rm -v "$PWD:/data" ghcr.io/rustledger/rustledger:1.0.0 /data/ledger.beancount
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/rustledger/rustledger/releases) for:
- Linux (x86_64, ARM64, glibc and musl)
- macOS (Intel and Apple Silicon)
- Windows (x86_64, ARM64)

### As a Library

```bash
cargo add rustledger-core rustledger-parser rustledger-loader
```

### WebAssembly (npm)

```bash
npm install @rustledger/wasm
```

### MCP Server

```bash
npm install -g @rustledger/mcp-server
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `rledger-check` | Validate ledger files with detailed error messages |
| `rledger-format` | Auto-format beancount files |
| `rledger-query` | Run BQL queries (interactive shell or one-shot) |
| `rledger-report` | Generate balance, account, and statistics reports |
| `rledger-doctor` | Debugging tools: context, linked transactions, missing opens |
| `rledger-extract` | Import transactions from CSV/OFX bank statements |
| `rledger-price` | Fetch commodity prices from online sources |

### Examples

```bash
# Validate with plugins
rledger-check --native-plugin auto_accounts ledger.beancount
rledger-check --native-plugin pedantic ledger.beancount

# Format in place
rledger-format --in-place ledger.beancount

# Interactive query shell with readline and history
rledger-query ledger.beancount

# One-shot query
rledger-query ledger.beancount "SELECT date, narration WHERE account ~ 'Expenses:Food'"

# Reports
rledger-report ledger.beancount balances
rledger-report ledger.beancount accounts
rledger-report ledger.beancount stats

# Debugging
rledger-doctor ledger.beancount context 42        # Show context around line 42
rledger-doctor ledger.beancount linked ^trip-2024 # Find linked transactions
rledger-doctor ledger.beancount missing           # Find missing Open directives
```

### Python Beancount Compatibility

For users migrating from Python beancount, the `bean-*` commands are also available:

```bash
bean-check ledger.beancount
bean-format ledger.beancount
bean-query ledger.beancount "SELECT ..."
bean-report ledger.beancount balances
bean-doctor ledger.beancount context 42
```

### Shell Completions

All CLI commands support generating shell completions:

```bash
# Bash (add to ~/.bashrc)
rledger-check --generate-completions bash >> ~/.bashrc

# Zsh (add to ~/.zshrc)
rledger-check --generate-completions zsh >> ~/.zshrc

# Fish
rledger-check --generate-completions fish > ~/.config/fish/completions/rledger-check.fish

# PowerShell
rledger-check --generate-completions powershell >> $PROFILE
```

Generate completions for each command you use (`rledger-check`, `rledger-query`, etc.).

## Library Usage

```rust
use rustledger_loader::load;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let result = load(Path::new("ledger.beancount"))?;

    println!("Loaded {} directives", result.directives.len());

    for error in &result.errors {
        eprintln!("{}", error);
    }

    Ok(())
}
```

## Importing Bank Statements

rustledger includes an import framework for extracting transactions from CSV and OFX files:

```rust
use rustledger_importer::ImporterConfig;

let config = ImporterConfig::csv()
    .account("Assets:Bank:Checking")
    .currency("USD")
    .date_column("Date")
    .narration_column("Description")
    .amount_column("Amount")
    .date_format("%m/%d/%Y")
    .build();

let result = config.extract_from_string(csv_content)?;
for directive in result.directives {
    println!("{}", directive);
}
```

Supports:
- CSV with configurable columns, delimiters, date formats
- Separate debit/credit columns
- OFX/QFX bank statement files
- Currency symbols, parentheses for negatives, thousand separators

## Crates

| Crate | Description |
|-------|-------------|
| [`rustledger-core`](crates/rustledger-core) | Core types: Amount, Position, Inventory, Directives |
| [`rustledger-parser`](crates/rustledger-parser) | Lexer and parser with error recovery |
| [`rustledger-loader`](crates/rustledger-loader) | File loading, includes, options |
| [`rustledger-booking`](crates/rustledger-booking) | Interpolation and booking engine |
| [`rustledger-validate`](crates/rustledger-validate) | 30 validation error codes |
| [`rustledger-query`](crates/rustledger-query) | BQL query engine |
| [`rustledger-plugin`](crates/rustledger-plugin) | Native and WASM plugin system |
| [`rustledger-importer`](crates/rustledger-importer) | CSV/OFX import framework |
| [`rustledger`](crates/rustledger) | Command-line tools |
| [`rustledger-wasm`](crates/rustledger-wasm) | WebAssembly library target |

## Features

### Parser

- All 12 directive types (transaction, balance, open, close, commodity, pad, event, query, note, document, custom, price)
- Cost specifications: `{100 USD}`, `{{100 USD}}`, `{100 # 5 USD}`, `{*}`
- Price annotations: `@ 100 USD`, `@@ 1000 USD`
- Arithmetic expressions: `(40.00/3 + 5) USD`
- Multi-line strings with `"""..."""`
- All transaction flags: `* ! P S T C U R M`
- Metadata with 6 value types
- Error recovery (continues parsing after errors)

### Booking Methods

| Method | Description |
|--------|-------------|
| `STRICT` | Lots must match exactly (default) |
| `STRICT_WITH_SIZE` | Exact-size matches accept oldest lot |
| `FIFO` | First in, first out |
| `LIFO` | Last in, first out |
| `HIFO` | Highest cost first |
| `AVERAGE` | Average cost basis |
| `NONE` | No cost tracking |

### Built-in Plugins (14)

| Plugin | Description |
|--------|-------------|
| `implicit_prices` | Generate price entries from transaction costs |
| `check_commodity` | Validate commodity declarations |
| `auto_accounts` | Auto-generate Open directives |
| `leafonly` | Error on postings to non-leaf accounts |
| `noduplicates` | Hash-based duplicate transaction detection |
| `onecommodity` | Single commodity per account |
| `unique_prices` | One price per day per commodity pair |
| `check_closing` | Zero balance assertion on account close |
| `close_tree` | Close descendant accounts |
| `coherent_cost` | Enforce cost OR price (not both) |
| `sellgains` | Cross-check capital gains against sales |
| `pedantic` | Enable all strict validations |
| `unrealized` | Calculate unrealized gains |
| `nounused` | Warn on unused accounts |

### Options (28 supported)

- Account prefixes (`name_assets`, `name_liabilities`, etc.)
- Equity accounts (`account_previous_balances`, `account_unrealized_gains`, etc.)
- Tolerance settings (`inferred_tolerance_default` with wildcards)
- Booking method, document directories, and more

## Performance

rustledger is approximately **10x faster** than Python beancount:

| Operation | Python beancount | rustledger | Speedup |
|-----------|------------------|------------|---------|
| Parse 10K transactions | ~800ms | ~80ms | 10x |
| Full validation | ~1.2s | ~120ms | 10x |
| BQL query | ~200ms | ~20ms | 10x |

*Benchmarks on M1 MacBook Pro with a real-world 10,000 transaction ledger.*

## Formal Verification

Core algorithms are formally specified and verified using TLA+:

- **19 TLA+ specifications** covering inventory management, booking methods, validation rules
- **Inductive invariants** prove conservation of units across all operations
- **Model checking** explores millions of states to find edge cases
- **Refinement proofs** verify Rust implementation matches specifications

```bash
# Run all TLA+ model checks
just tla-all

# Check specific specification
just tla-check Conservation
```

## Development

### With Nix (recommended)

```bash
# Enter development shell with all tools
nix develop

# Run tests
cargo test --all-features

# Run lints
cargo clippy --all-features

# Format code
cargo fmt
```

### Without Nix

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/rustledger/rustledger
cd rustledger
cargo build --release
```

### Running Tests

```bash
# All tests
cargo test --all-features

# Specific crate
cargo test -p rustledger-parser

# With coverage
cargo llvm-cov --all-features
```

## Compatibility

rustledger is fully compatible with Python beancount. It can parse and validate any valid beancount file. The `bean-*` command aliases are included by default for easy migration.

Known differences:
- Some edge cases in expression evaluation may differ slightly
- Plugin system uses native Rust or WASM (Python plugins not supported)

## License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

Before submitting:
1. Run `cargo test --all-features`
2. Run `cargo clippy --all-features`
3. Run `cargo fmt`

See [CLAUDE.md](CLAUDE.md) for code standards and architecture overview.
