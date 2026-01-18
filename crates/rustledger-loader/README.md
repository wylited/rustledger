# rustledger-loader

Beancount file loader with include resolution, options parsing, and binary caching.

## Features

- Recursive `include` directive resolution
- Option parsing and validation
- Plugin directive collection
- Binary cache for faster subsequent loads
- Path traversal protection

## Example

```rust
use rustledger_loader::load_file;

let result = load_file("ledger.beancount")?;

println!("Loaded {} directives", result.directives.len());
println!("Options: {:?}", result.options);
println!("Errors: {:?}", result.errors);
```

## Cargo Features

- `cache` (default) - Enable rkyv-based binary caching for faster loads

## License

GPL-3.0
