# Acme

Acme is a command-line tool that turns merged pull requests into release
notes. This file is for coding agents; people start at
[CONTRIBUTING.md](CONTRIBUTING.md).

## Project layout

- `src/`: the command-line tool.
- `packages/api/`: the HTTP API, with its own
  [AGENTS.md](packages/api/AGENTS.md).
- How the two fit together:
  [docs/architecture.md](docs/architecture.md#module-boundaries).

## Build and test

Run all three before you finish a change, and fix what fails:

```sh
cargo build --locked
cargo test --locked
cargo clippy --all-targets -- -D warnings
```

## Code style

- Format with `cargo fmt`.
- No `unwrap()` outside tests.

## Security

- Never commit secrets or `.env` files.
- Ask before adding a dependency.
