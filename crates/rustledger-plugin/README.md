# rustledger-plugin

Beancount plugin system with 20 native plugins and WASM support.

## Native Plugins

| Plugin | Description |
|--------|-------------|
| `auto_accounts` | Auto-generate Open directives |
| `auto_tag` | Automatically tag transactions |
| `check_average_cost` | Validate average cost bookings |
| `check_closing` | Zero balance on account close |
| `check_commodity` | Validate commodity declarations |
| `check_drained` | Ensure accounts drained before close |
| `close_tree` | Close descendant accounts |
| `coherent_cost` | Enforce cost OR price (not both) |
| `commodity_attr` | Validate commodity attributes |
| `currency_accounts` | Enforce currency constraints |
| `document_discovery` | Auto-discover document files |
| `implicit_prices` | Generate prices from costs |
| `leafonly` | Error on non-leaf account postings |
| `noduplicates` | Detect duplicate transactions |
| `nounused` | Warn on unused accounts |
| `onecommodity` | Single commodity per account |
| `pedantic` | Enable all strict validations |
| `sellgains` | Cross-check capital gains |
| `unique_prices` | One price per day per pair |
| `unrealized` | Calculate unrealized gains |

## Example

```rust
use rustledger_plugin::{NativePluginRegistry, run_plugin};

let registry = NativePluginRegistry::new();
let plugin = registry.get("auto_accounts")?;
let result = run_plugin(plugin, &directives)?;
```

## Cargo Features

- `wasm-runtime` (default) - WASM plugin support via Wasmtime
- `python-plugins` - Run Python beancount plugins via WASI sandbox

## License

GPL-3.0
