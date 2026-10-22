# Fiex Plugin SDK

Independent SDKs for developing plugins for [Fiex](https://github.com/kevinhuang001/fiex).

## 📖 Start here / 从这里开始

**第一次开发 Rust 插件？** 从这份图文式步骤教程开始，不需要先理解 JSON-RPC：

➡️ **[Rust 插件 SDK：从零开始（中文新手教程）](rust/README.md)**

The Chinese walkthrough takes you from an empty Rust project to a working native plugin, real-process tests, a `manifest.json`, cross-platform `.fiexplugin` packaging and GitHub Release installation.

### Which SDK should I use? / 该选哪个 SDK？

| 目录                      | 用途                                                                                                     |
| ------------------------- | -------------------------------------------------------------------------------------------------------- |
| [`rust/`](rust/README.md) | Native 插件 / Native process SDK：Rust 命令、文件系统适配器、宿主托管存储与凭据，采用 stdio JSON-RPC 2.0 |
| [`ui/`](ui/src/index.ts)  | UI SDK：在沙箱 iframe 中开发插件面板，通过 MessageChannel 与宿主通信                                     |

You can build a native-only plugin, a UI panel, or use both. A native plugin runs as a separate executable and must not write logs to stdout (use stderr instead).

## Quick SDK checks / 检查 SDK

```sh
# Rust SDK
cargo fmt --manifest-path rust/Cargo.toml --check
cargo test --manifest-path rust/Cargo.toml
cargo build --manifest-path rust/Cargo.toml --example mock_plugin

# Test the actual mock plugin process (Python 3 required)
python rust/examples/check_plugin.py --binary rust/target/debug/examples/mock_plugin --plugin-id org.fiex.mock --method mock.echo

# Browser-side UI SDK
npm --prefix ui install
npm --prefix ui run build
npm --prefix ui test
```

The Rust SDK currently requires **Rust 1.90+**. For full explanations of each command, see the [beginner tutorial](rust/README.md).

## Real plugin examples / 真正的插件项目

See [fiex-official-plugins](https://github.com/kevinhuang001/fiex-official-plugins) for examples of Checksum, Archive, Git, SMB, and WebDAV plugins, including GitHub Actions packaging.

The APIs are **experimental** and may evolve with the Fiex Core development branch. Fiex does not bundle any official plugins or exempt official plugins from permission approval. **Native plugins currently do not run inside an OS-level sandbox.** Only install plugins you trust.

Pin the SDK with a Git submodule when developing a plugin so that your SDK version matches the version tested with your host and plugin project.
