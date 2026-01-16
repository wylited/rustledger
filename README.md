<div align="center">

# rustledger

**A blazing-fast Rust implementation of [Beancount](https://beancount.github.io/)**

Parse and validate your ledger faster than Python beancount.

[![CI](https://github.com/rustledger/rustledger/actions/workflows/ci.yml/badge.svg)](https://github.com/rustledger/rustledger/actions/workflows/ci.yml)
[![GitHub Release](https://img.shields.io/github/v/release/rustledger/rustledger)](https://github.com/rustledger/rustledger/releases)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

</div>

---

## Why rustledger?

| | |
|---|---|
| **Much faster** | Parse and validate large ledgers in milliseconds ([see benchmarks](#performance)) |
| **Single binary** | No Python, no dependencies, just download and run |
| **Drop-in replacement** | Compatible `bean-*` CLI commands for easy migration |
| **Full compatibility** | Parses any valid beancount file |

## Install

| Platform | Command |
|----------|---------|
| **Script** | `curl -sSfL rustledger.github.io/i \| sh` |
| **macOS** | `brew install rustledger/rustledger/rustledger` |
| **Ubuntu/Debian** | `sudo add-apt-repository ppa:robcohen/rustledger && sudo apt install rustledger` |
| **Fedora/RHEL** | `sudo dnf copr enable rustledger/rustledger && sudo dnf install rustledger` |
| **Arch** | `yay -S rustledger-bin` |
| **Windows** | `scoop bucket add rustledger https://github.com/rustledger/scoop-rustledger && scoop install rustledger` |
| **Cargo** | `cargo binstall rustledger` or `cargo install rustledger` |
| **Nix** | `nix run github:rustledger/rustledger` |
| **Docker** | `docker run --rm -v "$PWD:/data" ghcr.io/rustledger/rustledger /data/ledger.beancount` |
| **Binaries** | [GitHub Releases](https://github.com/rustledger/rustledger/releases) |
| **npm** | `npm install @rustledger/wasm` (WebAssembly) |

## Quick Start

```bash
rledger-check ledger.beancount
rledger-query ledger.beancount "SELECT account, SUM(position) GROUP BY account"
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `rledger-check` | Validate ledger files with detailed error messages |
| `rledger-query` | Run BQL queries (interactive shell or one-shot) |
| `rledger-format` | Auto-format beancount files |
| `rledger-report` | Generate balance, account, and statistics reports |
| `rledger-doctor` | Debugging tools for ledger issues |
| `rledger-extract` | Import transactions from CSV/OFX bank statements |
| `rledger-price` | Fetch commodity prices from online sources |

Python beancount users can also use `bean-check`, `bean-query`, etc.

<details>
<summary><strong>CLI examples</strong></summary>

```bash
# Validate with plugins
rledger-check --native-plugin auto_accounts ledger.beancount

# Interactive query shell
rledger-query ledger.beancount

# One-shot query
rledger-query ledger.beancount "SELECT date, narration WHERE account ~ 'Expenses:Food'"

# Reports
rledger-report ledger.beancount balances
rledger-report ledger.beancount stats

# Format in place
rledger-format --in-place ledger.beancount
```

</details>

## Crates

| Crate | Description |
|-------|-------------|
| `rustledger` | CLI tools (rledger-check, rledger-query, etc.) |
| `rustledger-core` | Core types: Amount, Position, Inventory |
| `rustledger-parser` | Lexer and parser with error recovery |
| `rustledger-loader` | File loading and includes |
| `rustledger-booking` | Interpolation and 7 booking methods |
| `rustledger-validate` | 26 validation error codes |
| `rustledger-query` | BQL query engine |
| `rustledger-plugin` | 20 built-in plugins + Python plugin support |
| `rustledger-importer` | CSV/OFX import framework |
| `rustledger-wasm` | WebAssembly bindings for JavaScript/TypeScript |

<details>
<summary><strong>Booking methods (7)</strong></summary>

| Method | Description |
|--------|-------------|
| `STRICT` | Lots must match exactly (default) |
| `STRICT_WITH_SIZE` | Exact-size matches accept oldest lot |
| `FIFO` | First in, first out |
| `LIFO` | Last in, first out |
| `HIFO` | Highest cost first |
| `AVERAGE` | Average cost basis |
| `NONE` | No cost tracking |

</details>

<details>
<summary><strong>Built-in plugins (20)</strong></summary>

| Plugin | Description |
|--------|-------------|
| `auto_accounts` | Auto-generate Open directives |
| `auto_tag` | Automatically tag transactions |
| `check_average_cost` | Validate average cost bookings |
| `check_closing` | Zero balance assertion on account close |
| `check_commodity` | Validate commodity declarations |
| `check_drained` | Ensure accounts are drained before close |
| `close_tree` | Close descendant accounts |
| `coherent_cost` | Enforce cost OR price (not both) |
| `commodity_attr` | Validate commodity attributes |
| `currency_accounts` | Enforce currency constraints on accounts |
| `document_discovery` | Auto-discover document files |
| `implicit_prices` | Generate price entries from transaction costs |
| `leafonly` | Error on postings to non-leaf accounts |
| `noduplicates` | Hash-based duplicate transaction detection |
| `nounused` | Warn on unused accounts |
| `onecommodity` | Single commodity per account |
| `pedantic` | Enable all strict validations |
| `sellgains` | Cross-check capital gains against sales |
| `unique_prices` | One price per day per commodity pair |
| `unrealized` | Calculate unrealized gains |

**Python plugins**: Run existing Python beancount plugins via CPython-WASI sandbox.

</details>

## Performance

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/rustledger/rustledger/benchmarks/.github/badges/validation-chart.svg">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/rustledger/rustledger/benchmarks/.github/badges/validation-chart.svg">
  <img alt="Validation Benchmark" src="https://raw.githubusercontent.com/rustledger/rustledger/benchmarks/.github/badges/validation-chart.png" width="100%">
</picture>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/rustledger/rustledger/benchmarks/.github/badges/balance-chart.svg">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/rustledger/rustledger/benchmarks/.github/badges/balance-chart.svg">
  <img alt="Balance Report Benchmark" src="https://raw.githubusercontent.com/rustledger/rustledger/benchmarks/.github/badges/balance-chart.png" width="100%">
</picture>

<sub>Benchmarks run nightly on 10K transaction ledgers. [View workflow →](https://github.com/rustledger/rustledger/actions/workflows/bench.yml)</sub>

## Contributing

See [CLAUDE.md](CLAUDE.md) for architecture and development setup.

## License

[GPL-3.0](LICENSE)
