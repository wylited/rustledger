# rustledger-lsp

Language Server Protocol (LSP) implementation for Beancount.

## Status

**Work in Progress** - This crate is under active development and not yet functional.

## Features (Planned)

- Real-time syntax error diagnostics
- Autocompletion for accounts, currencies, payees
- Go-to-definition for accounts
- Hover information (account balances, metadata)
- Document symbols (outline view)
- Code actions (quick fixes)

## Usage

```bash
# Start the LSP server (communicates via stdio)
rledger-lsp

# Check version
rledger-lsp --version
```

## Editor Integration

### VS Code

Coming soon.

### Neovim

```lua
require('lspconfig').rledger.setup {
  cmd = { 'rledger-lsp' },
  filetypes = { 'beancount' },
}
```

### Emacs

```elisp
(use-package lsp-mode
  :hook (beancount-mode . lsp))
```

## Architecture

Based on rust-analyzer patterns:
- Main loop handles LSP messages
- Notifications processed synchronously
- Requests dispatched to threadpool
- Revision-based cancellation for stale requests

## License

GPL-3.0-only
