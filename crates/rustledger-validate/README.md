# rustledger-validate

Beancount validation with 27 error codes for ledger correctness.

## Error Categories

| Range | Category |
|-------|----------|
| E1xxx | Account errors (not opened, already closed, etc.) |
| E2xxx | Balance/pad errors |
| E3xxx | Transaction errors (unbalanced, no postings) |
| E4xxx | Inventory/lot errors |
| E5xxx | Currency errors |
| E6xxx | Metadata errors |
| E7xxx | Option errors |
| E8xxx | Document errors |
| E10xxx | Date warnings |

## Example

```rust
use rustledger_validate::{validate, ValidationError};

let errors = validate(&directives, &options);

for error in errors {
    eprintln!("{}: {}", error.code(), error.message());
}
```

## Features

- Parallel validation with rayon
- Configurable error severity
- Rich error messages with source locations

## License

GPL-3.0
