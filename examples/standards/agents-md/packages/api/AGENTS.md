# Acme API

For files under `packages/api/`, this file is the closest AGENTS.md and takes
precedence over the [root one](../../AGENTS.md).

## Build and test

```sh
cargo test -p acme-api --locked
```

The API's request types are described in
[docs/architecture.md](../../docs/architecture.md#the-api).
