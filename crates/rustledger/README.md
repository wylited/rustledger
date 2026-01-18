# rustledger

Drop-in replacement for Beancount CLI tools. Pure Rust, 10-30x faster.

## Commands

| Command | Description |
|---------|-------------|
| `rledger-check` | Validate ledger files |
| `rledger-query` | Run BQL queries |
| `rledger-format` | Auto-format beancount files |
| `rledger-report` | Generate reports (balances, stats) |
| `rledger-doctor` | Debug ledger issues |
| `rledger-extract` | Import from CSV/OFX |
| `rledger-price` | Fetch commodity prices |

## Compatibility

With default features, also installs `bean-*` commands for Python beancount compatibility:
- `bean-check`, `bean-query`, `bean-format`, `bean-report`, `bean-doctor`, `bean-extract`, `bean-price`

## Install

```bash
cargo install rustledger
# or without bean-* compatibility aliases:
cargo install rustledger --no-default-features
```

## Example

```bash
rledger-check ledger.beancount
rledger-query ledger.beancount "SELECT account, SUM(position) GROUP BY account"
rledger-format --in-place ledger.beancount
```

## Cargo Features

- `bean-compat` (default) - Include `bean-*` binaries
- `python-plugin-wasm` (default) - Enable Python plugin support

## License

GPL-3.0
