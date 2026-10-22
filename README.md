# Fiex Plugin SDK

The independent SDK for [Fiex](https://github.com/kevinhuang001/fiex).

- `rust/`: standalone native plugin process SDK with length-framed JSON-RPC 2.0 over stdio.
- `ui/`: TypeScript API for sandboxed iframe plugin panels, plus the host MessageChannel bridge.

The APIs remain experimental. The host enforces permission grants equally for all plugins; official plugins have no privilege exemptions.

## Test

```sh
cargo fmt --manifest-path rust/Cargo.toml --check
cargo test --manifest-path rust/Cargo.toml
npm --prefix ui install
npm --prefix ui run build
npm --prefix ui test
```

Use as a pinned Git submodule in [Fiex Core](https://github.com/kevinhuang001/fiex) and [Official Plugins](https://github.com/kevinhuang001/fiex-official-plugins) to guarantee matching SDK revisions.
