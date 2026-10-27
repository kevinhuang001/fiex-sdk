# Fiex Rust 插件 SDK：从零开始（新手教程）

> **适用对象：** 会复制命令、改几个文件，但还没写过 Fiex 插件的开发者。
>
> **当前状态：实验性 API。** 本教程以本仓库 `rust/src/lib.rs` 的**实际代码**和当前官方插件包为准，不依赖尚未实现的 `fiex-plugin` CLI 或拟议中的异步 API。宿主安装功能目前在 Fiex 的开发分支中迭代，发布前可能变化。

[返回 SDK 首页](../README.md) · [实际可用的最小示例](examples/mock_plugin.rs) · [真实进程测试工具](examples/check_plugin.py) · [官方插件源码](https://github.com/kevinhuang001/fiex-official-plugins)

## 文件和网络：直接用这些接口

插件不能自己调用 `std::fs`、建立 Socket 或启动子进程。用户在 Fiex 中激活插件并逐项同意后，由 Core 代为访问资源。SDK 已经处理分块、并发和 IPC，不需要学习 buffer 或共享内存。

在 `sdk::serve` 的回调里使用本次请求的 `ctx`：

```rust,no_run
# use fiex_plugin_sdk::{self as sdk, RpcError};
# fn example(ctx: &sdk::RequestContext) -> Result<(), Box<dyn std::error::Error>> {
let files = ctx.files();
// 协议 4.0 只接受 ResourceRef，不接受宿主路径。
let target = ctx.target().ok_or("missing target")?;
let text = files.read_to_string(&target.reference)?;
files.write(&target.reference, b"updated")?; // 修改本次操作授权的文件
// 新文件先由 Core 在已授权目录下分配一个身份。
let directory = ctx.directory().ok_or("missing directory")?;
let created = files.child(&directory.reference, "new.txt")?;
files.create(&created, b"hello")?; // 已存在则报错
# Ok(()) }
```

大文件使用标准 Rust 流式接口，内存占用不会随文件大小增长：

```rust,no_run
# use fiex_plugin_sdk::{self as sdk, RpcError};
# use std::io::Write;
# fn example(ctx: &sdk::RequestContext) -> Result<(), Box<dyn std::error::Error>> {
let files = ctx.files();
let target = ctx.target().ok_or("missing target")?;
let mut input = files.open_read(&target.reference)?;
let directory = ctx.directory().ok_or("missing directory")?;
let destination = files.child(&directory.reference, "copy.bin")?;
let mut output = files.begin_write(&destination, true)?;
std::io::copy(&mut input, &mut output)?;
output.commit()?;
# Ok(()) }
```

`begin_write` 后的数据由 Core 暂存。只有 `commit` 成功才会替换目标；原文件发生变化时返回冲突。没有提交就释放 writer，会自动取消暂存。`read` 最多读取 64 MiB；更大文件用 `open_read`。文件访问限于宿主为本次操作授权的所选文件或目录。协议 4.0 的公开输入是 `ResourceRef`：系统事件从 `ctx.targets()` 取得，文件/目录选择框也返回 `ResourceContext`；不要再传宿主路径。未提交的写入在操作结束时取消。

普通文件接口集中在 `ctx.files()`：`read`、`read_to_string`、`open_read`、`write`、`create`、`begin_write`、`list`、`metadata`、`child`、`create_dir`、`remove`、`rename`。`open_read` 返回实现 `Read + Seek + Send + 'static` 的 `FileReader`，`begin_write` 返回实现 `Write + Seek + Send + 'static` 的 `FileWriter`，并要求显式 `commit()`。Reader/Writer 自己持有 SDK 内部通信句柄，可以先 `drop(files)` 再继续使用；`'static` 只表示类型不依赖临时借用，文件授权仍由 Core 按操作作用域校验。SDK 自动对小读取使用 256 KiB IPC 缓存，对大块读取复用最多 4 MiB 的共享缓冲；大块写入复用 256 KiB 共享缓冲。共享缓冲不可分配时退回有界 IPC。缓冲占用不随文件长度增长。

HTTP 调用也不接触底层数据缓冲区：

```rust,no_run
# use fiex_plugin_sdk::{self as sdk, RpcError};
# fn example(ctx: &sdk::RequestContext) -> Result<(), Box<dyn std::error::Error>> {
let http = ctx.http();
let text = http.get("https://example.com/info").send()?.error_for_status()?.text()?;
let data: serde_json::Value = http.post("https://example.com/api")
    .bearer_auth("token")
    .json(&serde_json::json!({"name": "demo"}))?
    .send()?.error_for_status()?.json()?;
# Ok(()) }
```

可设置 `header`、`timeout`、`basic_auth` 和请求 `body`。HTTP 响应实现 `Read`，可以流式处理下载；`bytes`、`text`、`json` 最多收集 64 MiB，内联请求体最多 1 MiB。重定向由 Core 逐跳校验授权，不会把认证头带到不同来源。

HTTP 和 TCP 共用 manifest 的 `network.domains`，包括精确域名与 IP，不区分 HTTP/TCP 授权。批准主机允许访问其有效端口；localhost 也需要授权。TCP 只用于非 HTTP 协议，返回标准字节流：

```rust,no_run
# use fiex_plugin_sdk::{self as sdk, RpcError};
# use std::io::{Read, Write};
# fn example(ctx: &sdk::RequestContext) -> Result<(), Box<dyn std::error::Error>> {
let mut stream = ctx.tcp().connect("192.168.1.100", 445)?;
stream.write_all(b"request")?;
let mut response = [0; 4];
stream.read_exact(&mut response)?;
# Ok(()) }
```

`TcpStream` 实现 `Read + Write + Send + 'static`，由 Core 创建、授权和关闭。

长时间任务使用显式后台任务 API，任务拥有独立生命周期并继承本次操作已批准的授权范围：

```rust,no_run
# use fiex_plugin_sdk::{self as sdk, RpcError};
# fn example(ctx: &sdk::RequestContext) -> Result<(), Box<dyn std::error::Error>> {
let reference = ctx.target().ok_or("missing target")?.reference.clone();
let task = ctx.spawn_task(move |task| {
    let files = task.files();
    let mut reader = files.open_read(&reference)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let mut block = [0u8; 65536];
    use std::io::Read;
    while reader.read(&mut block).map_err(|error| RpcError::internal(error.to_string()))? > 0 {
        if task.is_cancelled() {
            return Err(RpcError::cancelled());
        }
    }
    Ok(serde_json::Value::Null)
})?;
// 请求可以先返回 task.id()，任务继续在后台运行。
let _ = task.id();
# Ok(()) }
```

`TaskHandle` 提供 `cancel()`、`is_finished()`、`status()`、`progress()` 和 `wait()`；任务闭包返回的 JSON 可通过 `wait()` 取得最终结果。Core 会把任务注册到状态中心，用户可以在那里取消；SDK 收到取消通知后设置协作取消标记，任务应轮询 `is_cancelled()`。SDK 默认最多同时运行 8 个后台任务，并在任务完成、取消或失败后通知 Core 回收资源。`spawn_task_with_label` 可以给状态中心提供比方法名更友好的标题；`spawn_internal_task` 用于 SMB 读取流这类不应出现在状态中心的内部资源任务。`ctx.invoke` 仍可用于通用宿主能力；存储、凭据、终端和文件优先使用各自的类型化 API。普通插件不需要再接触 `HostProxy`、worker 或 IPC 细节。

外部命令属于另一种授权：`process_run` 通过 Host 执行 PATH 中已批准的程序。该程序在插件沙箱外以当前用户身份运行，可能访问其他文件、联网或启动其他程序。不要把命令授权理解成文件和网络 SDK 的范围限制。

## 你最终会做出什么？

我们要做一个叫 **Hello Fiex** 的插件。向它发送 `hello.greet` 命令和名字 `新手`，它会返回：

```json
{ "message": "你好，新手！" }
```

你会依次完成：**安装工具 → 创建项目 → 写 1 个 Rust 文件 → 编译 → 调用真实插件进程 → 生成 .fiexplugin 包 → 发布到 GitHub 供 Fiex 安装。**

先记住三个词：

| 名称              | 大白话解释                                                |
| ----------------- | --------------------------------------------------------- |
| **SDK**           | 帮你处理插件与 Fiex 之间的消息收发；你只写自己的功能      |
| **Core / 宿主**   | Fiex 主程序，负责启动插件、校验授权、显示工具栏和插件页面 |
| **manifest.json** | 插件的“身份证”，说明名字、入口程序、界面贡献和需要的权限  |

**Rust SDK 只负责 Native 插件（独立进程）。** 如果要开发 iframe 中的插件界面，应另用同仓库的 [UI SDK](../ui/src/index.ts)；一个插件也可以同时拥有 Native 后端和 UI 界面。

## 0. 安装开发工具（只做一次）

需要：

- **Git**：下载 SDK、固定版本。安装地址：[git-scm.com](https://git-scm.com/downloads)
- **Rust（1.90 或更新）**：包含编译器和 `cargo`。安装地址：[rustup.rs](https://rustup.rs)
- **Python 3**：运行本文的跨平台测试及打包脚本。安装地址：[python.org](https://www.python.org/downloads/)；不是插件的运行时依赖
- 一个文本编辑器，例如 VS Code

打开 Windows PowerShell、macOS 终端或 Linux 终端，分别执行：

```sh
git --version
rustc --version
cargo --version
python --version
```

看到版本号就说明命令找到了。Windows 如果提示找不到 `python`，可试 `py -3 --version`，下文 `python` 同样改为 `py -3`。如果 Rust 版本低于 1.90，执行 `rustup update stable`。Windows 安装 Rust 时可能还需要按 rustup 提示安装 MSVC C++ 构建工具。

### 想先确认 SDK 本身没坏？

```sh
git clone https://github.com/kevinhuang001/fiex-sdk.git
cd fiex-sdk
cargo test --manifest-path rust/Cargo.toml
cargo build --manifest-path rust/Cargo.toml --example mock_plugin
python rust/examples/check_plugin.py --binary rust/target/debug/examples/mock_plugin --plugin-id org.fiex.mock --method mock.echo
```

最后应看到“初始化成功”“方法调用成功”“正常退出”。在 Windows 上，测试工具会自动为未写后缀的可执行文件路径补上 `.exe`。

这个 `mock_plugin` 是仓库内**真的能编译和运行**的例子，不是伪代码。测完后回到自己想放项目的文件夹，再做下一步。

## 1. 新建自己的插件项目

任选一个工作目录，**不要在刚克隆的 SDK 仓库内部**执行下面的 `cargo new`：

```sh
cargo new --bin fiex-plugin-hello
cd fiex-plugin-hello
git submodule add https://github.com/kevinhuang001/fiex-sdk.git sdk
```

文件夹会长这样（省略自动生成的文件）：

```text
fiex-plugin-hello/
├── Cargo.toml       # Rust 项目依赖
├── src/
│   └── main.rs      # 你的插件功能
└── sdk/             # Git submodule：固定版本的 Fiex SDK
    └── rust/
        └── src/lib.rs
```

**submodule 是什么？** 就是把 SDK 的一个确定 Git 版本关联到你的插件项目。开发时正常编辑自己的文件即可，不需要复制或修改 SDK 源码。其他人克隆你的插件后可以执行：

```sh
git submodule update --init --recursive
```

如果你之前已经自己克隆过 SDK，也可以用本地 `path` 依赖，但发布项目时**推荐**上面的 submodule 方式，避免依赖路径指向你电脑上独有的位置。

## 2. 编辑 Cargo.toml（告诉 Rust 要用什么 SDK）

把项目根目录的 `Cargo.toml` **完整替换**成：

```toml
[package]
name = "fiex-plugin-hello"
version = "0.1.0"
edition = "2021"
rust-version = "1.90"

[dependencies]
fiex-plugin-sdk = { path = "sdk/rust" }
serde_json = "1"
```

逐行解释：

- `name`：编译出来的文件名来自这里，例如 `fiex-plugin-hello.exe`。
- `fiex-plugin-sdk`：就是当前子模块 `sdk/rust` 中的 Rust SDK。
- `serde_json`：方便接收 JSON 参数、返回 JSON 结果。
- `rust-version`：SDK 当前要求的 Rust 最低版本。

不用把 SDK 的 `lib.rs` 复制到自己的项目，也不用写自己的 IPC 收发循环。

## 3. 编辑 src/main.rs（真正的插件功能）

把 `src/main.rs` **完整替换**成下面这段代码：

```rust
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

// Fiex 向插件发送命令时，就会调用这个函数。
fn handle(ctx: sdk::RequestContext) -> Result<Value, RpcError> {
    let method = ctx.method();
    let params = ctx.params();
    match method {
        "hello.greet" => {
            // 用户没传名字，就使用默认名字 "Fiex"。
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Fiex");

            // 这是返回给 Fiex 的 JSON；不是在终端打印日志。
            Ok(json!({ "message": format!("你好，{name}！") }))
        }
        // 不认识的命令必须返回错误，不要假装执行成功。
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    // 这个 ID 必须与后面 manifest.json 的 id 完全一致。
    if let Err(error) = sdk::serve("org.example.hello", handle) {
        // 插件 stdout 专供 IPC 使用；调试信息只能写到 stderr。
        eprintln!("Hello Fiex 插件出错：{error}");
        std::process::exit(1);
    }
}
```

代码里最重要的只有三件事：

1. `sdk::serve(...)` 让 SDK 负责插件的启动握手、接收请求和发送响应。
2. `"hello.greet"` 是**命令名**，宿主用这个名字找到对应功能。
3. `Ok(json!(...))` 是成功返回值；`Err(RpcError::...)` 是失败返回值。

**不要用 `println!` 打印调试信息！** 插件用标准输出（`stdout`）跟 Fiex 交换协议消息。`println!` 可能破坏协议，让插件看起来像是“莫名其妙地断开”。要记录日志，请用 `eprintln!`。

## 4. 编译、运行真实进程测试

保持终端在 `fiex-plugin-hello/` 项目根目录下：

```sh
cargo fmt
cargo test
cargo build
```

现在开始真正测试插件，而不是直接在终端输入 `cargo run`：

```sh
python sdk/rust/examples/check_plugin.py --binary target/debug/fiex-plugin-hello --plugin-id org.example.hello --method hello.greet
```

测试器会依次做：

1. 启动你刚刚编译出来的可执行文件。
2. 发起 `plugin.initialize` 握手，协议版本为 `4.0`。
3. 调用 `hello.greet`，默认发送 `{"name":"新手"}`。
4. 发起 `plugin.shutdown` 并确认正常退出。

成功时类似这样：

```text
✅ 初始化成功： {'pluginId': 'org.example.hello', 'protocolVersion': '4.0', 'ready': True}
✅ 方法调用成功： {"message": "你好，新手！"}
✅ 正常退出，三项检查全部通过
```

如果你想换参数，可以加 `--params`，例如在 macOS/Linux 或 PowerShell 中：

```sh
python sdk/rust/examples/check_plugin.py --binary target/debug/fiex-plugin-hello --plugin-id org.example.hello --method hello.greet --params '{"name":"Kevin"}'
```

> Windows **CMD** 对引号的规则不同；如果这条参数命令报错，先用上一条**不带** `--params` 的命令完成测试即可。
>
> **为什么不直接 cargo run？** 因为插件不是普通命令行聊天程序，它要等待宿主发送带 `Content-Length` 的 JSON-RPC 请求。直接运行后等待输入是正常的，并不代表死机。

## 5. 创建 manifest.json（插件的身份证）

在 `fiex-plugin-hello/` 根目录新建 `manifest.json`：

```json
{
  "schemaVersion": 2,
  "id": "org.example.hello",
  "name": "Hello Fiex",
  "version": "0.1.0",
  "runtime": {
    "protocolVersion": "4.0",
    "entrypoints": { "windows-x86_64": "fiex-plugin-hello.exe" }
  },
  "permissions": {},
  "extensions": {
    "toolbar": [
      {
        "id": "hello.main",
        "title": "打个招呼",
        "icon": "message-circle",
        "command": "hello.greet"
      }
    ]
  },
  "author": {
    "name": "Your Name"
  }
}
```

**重要：上面 `entrypoints` 演示的是 Windows x86-64。** 如果你在别的平台开发，请把**整个** `entrypoints` 对象替换为表格中对应的唯一一行：

| 当前系统            | `entrypoints` 的键 | 值（编译后的文件名）    |
| ------------------- | ------------------ | ----------------------- |
| Windows x86-64      | `windows-x86_64`   | `fiex-plugin-hello.exe` |
| macOS Apple Silicon | `macos-aarch64`    | `fiex-plugin-hello`     |
| macOS Intel         | `macos-x86_64`     | `fiex-plugin-hello`     |
| Linux x86-64        | `linux-x86_64`     | `fiex-plugin-hello`     |

例如 Mac Apple Silicon 用 `"entrypoints": {"macos-aarch64": "fiex-plugin-hello"}`。一份包只放**当前平台**的可执行文件即可；要支持其他平台，必须在对应平台分别编译并打包，不能把 Windows 的 `.exe` 改个名字当作 macOS 程序。

还需理解几个字段：

- `id`：插件唯一 ID；必须跟 `sdk::serve` 里的字符串一致。
- `extensions.toolbar[].command`：让工具栏按钮调用 `hello.greet`。
- `permissions`：这个示例不通过宿主 API 访问文件、存储或凭据，因此是空对象。
- `author`：设置页面展示的作者信息，**不会赋予额外权限或官方身份**。

这里的命令返回值可以通过第 4 步的测试器看到。**工具栏执行命令不等于自动弹出一个显示 JSON 的窗口。** 想让用户看到漂亮的界面，需另做插件 UI 面板并通过 UI SDK 调用 Native 命令。参考 [官方 Archive 插件的 manifest 和 UI](https://github.com/kevinhuang001/fiex-official-plugins/tree/main/official/archive)。

## 6. 生成可以发布的 .fiexplugin 包

Fiex 的插件包后缀是 `.fiexplugin`，内部实际上是一个 ZIP。最简单的 Native 插件包只需要两个文件放在**压缩包根目录**：

```text
hello-windows-x86_64.fiexplugin
├── manifest.json
└── fiex-plugin-hello.exe
```

macOS/Linux 的包则使用对应的平台名和不带 `.exe` 的二进制文件。

### 6.1 编译正式版本

```sh
cargo build --release
```

`target/release/` 下会生成优化后的可执行文件。不要把 `target/debug/` 中的版本误当成正式发布版本。

### 6.2 写一个不用安装第三方库的打包脚本

在项目根目录新建 `pack.py`，**完整复制**：

```python
import json
import zipfile
from pathlib import Path

manifest = json.loads(Path("manifest.json").read_text(encoding="utf-8"))
entrypoints = manifest["entrypoints"]

if len(entrypoints) != 1:
    raise SystemExit("入门打包脚本要求 manifest 只保留当前平台的一个 entrypoint")

platform, filename = next(iter(entrypoints.items()))
if Path(filename).name != filename:
    raise SystemExit("这个入门示例要求可执行文件位于压缩包根目录")

binary = Path("target/release") / filename
if not binary.is_file():
    raise SystemExit(f"找不到 {binary}；请先执行 cargo build --release")

output = Path(f"hello-{platform}.fiexplugin")
with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
    archive.write("manifest.json", "manifest.json")
    archive.write(binary, filename)

print(f"已生成：{output}")
print("内部文件：", zipfile.ZipFile(output).namelist())
```

运行：

```sh
python pack.py
```

应该看到 `manifest.json` 和可执行文件恰好位于 ZIP 根目录。**不要把整个 `target/`、`sdk/` 或源代码目录打进包里。**

### 6.3 通过 GitHub Release 提供安装

Fiex 当前的安装入口是 **「设置 → 插件 → 从 GitHub 安装」**，从 GitHub Release 中选择对应平台的 `.fiexplugin` 文件，并在安装前显示权限申请。**仅把代码推送到 GitHub，不创建 Release，是无法被这个安装器发现的。**

按顺序做：

1. 在 GitHub 网站上创建一个**公开**仓库，比如 `YOUR_NAME/fiex-plugin-hello`。
2. 把当前项目提交并推送到该仓库（提交 `src/`、`Cargo.toml`、`Cargo.lock`、`manifest.json`、`pack.py`、`.gitmodules` 和 SDK submodule 的引用；**不要提交 `target/`**）。如果你不熟悉 Git，可以用 GitHub Desktop 完成提交和推送。
3. 在仓库的 **Releases → Draft a new release** 创建发布版本，例如标签 `v0.1.0`，上传 `hello-windows-x86_64.fiexplugin`（或对应平台的包）并发布。
4. 在支持此功能的 **Fiex 开发版本**中打开「设置 → 插件」，输入 `YOUR_NAME/fiex-plugin-hello`，点击获取插件，选中包，核对作者、插件 ID 和权限后再确认安装。

**提醒：** 自动打包多系统、多架构和发布 Release 的现成范例在 [fiex-official-plugins 的 GitHub Actions](https://github.com/kevinhuang001/fiex-official-plugins/blob/main/.github/workflows/ci.yml)。当你需要同时发布 Windows/macOS/Linux 时，建议照着它搭建 CI，而不是在一台电脑上手动伪造其他平台的包。

## 7. 下一步：真的使用宿主能力

Hello 插件不需要宿主权限。比如你想实现“点一下计数器就加 1，并且下次打开仍然保留”，应该使用宿主存储，而不是自己在 Fiex 目录里偷偷保存文件。

把 `src/main.rs` 替换成下面的进阶示例：

```rust
use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn main() {
    if let Err(error) = sdk::serve("org.example.hello", |ctx| {
        let method = ctx.method();
        match method {
            "hello.count" => {
                // 从宿主提供的、按插件隔离的存储读取计数。
                let storage = ctx.storage().scoped("default");
                let old: Option<u64> = storage.get("counter")?;
                let next = old.unwrap_or(0) + 1;
                storage.set("counter", &next)?;
                Ok(json!({ "count": next }))
            }
            _ => Err(RpcError::method_not_found(method)),
        }
    }) {
        eprintln!("插件出错：{error}");
        std::process::exit(1);
    }
}
```

还需要同步修改 manifest：

- `"permissions": {"storage": true}`（如果宿主版本改用能力表，以 manifest 校验结果为准）；
- 工具栏中的 `"command": "hello.count"`。

重新编译并重新打包。这个例子**必须通过真正的 Fiex 宿主**运行并由用户授权 `storage`；刚才的独立测试器只模拟插件握手，不实现 `host.storage.*` 回调，**不能**用来测试这个计数器。安装器当前对已安装的同 ID 插件不支持直接覆盖，需要先卸载再安装新版本。

如果你需要凭据，请使用类型化的 `ctx.secrets().scoped(scope).get/set` 并声明 `"secrets": true`。`SecretError` 会区分用户拒绝、系统钥匙串不可用、凭据缺失和数据格式错误；不要将密码硬编码到程序或输出到日志。

终端类插件使用 `ctx.terminal(session_id)`：`ready`、`output`、`input`、`size`、`closed`、`end`。它封装了 `host.terminal.*` 的 Base64 与字段名，SSH 插件已迁移到该 API。

SDK 当前公开的关键接口：

| API                                     | 作用                           | 什么时候用           |
| --------------------------------------- | ------------------------------ | -------------------- |
| `sdk::serve(id, handler)`               | 启动常驻插件、并发分发请求     | 所有 Native 插件     |
| `RequestContext::method/params`         | 当前请求的方法与参数           | 请求分发             |
| `RequestContext::files/http/tcp`        | 宿主托管的文件、HTTP、TCP 能力 | 访问用户授权资源     |
| `RequestContext::spawn_task`            | 创建独立生命周期的后台任务     | 大文件、长耗时操作   |
| `RequestContext::spawn_task_with_label` | 为状态中心提供任务标题         | 需要友好名称的长任务 |
| `TaskHandle::cancel/status/wait`        | 任务取消、状态与完成等待       | 需要进度或取消的界面 |
| `RpcError::method_not_found(...)`       | 不支持的命令                   | `match` 的默认分支   |
| `RpcError::invalid_params(...)`         | 请求缺少字段或参数错误         | 用户传来的参数无效   |
| `RpcError::internal(...)`               | 插件内部操作失败               | 底层 I/O 或其他错误  |
| `RequestContext::storage()`             | 类型化插件存储与 CAS           | 需 `storage` 权限    |
| `RequestContext::secrets()`             | 系统钥匙串凭据（区分错误状态） | 需 `secrets` 权限    |
| `RequestContext::terminal(session)`     | 终端会话的六个宿主操作         | 终端类插件           |
| `sdk::Providers`                        | 可选的标准事件 Provider 注册   | 需要类型化事件分发   |

具体签名以 [rust/src/context.rs](src/context.rs)、[rust/src/files/mod.rs](src/files/mod.rs) 和 [rust/src/tasks.rs](src/tasks.rs) 为准。SDK 的 IPC、共享内存和线程由 `rust/src/runtime/` 内部管理，不属于普通插件 API。

## 8. 常见问题：出错先看这里

| 现象                                                                | 最常见原因与解决方法                                                                  |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `cargo` / `rustc` 找不到                                            | Rust 没装好，或者安装后终端尚未重启                                                   |
| `failed to load source for dependency`                              | SDK submodule 未初始化，执行 `git submodule update --init --recursive`                |
| Rust 版本太低                                                       | 执行 `rustup update stable`，需 Rust 1.90+                                            |
| `cargo run` 好像卡住                                                | 正常：进程在等待宿主发 IPC 请求；用本文的 `check_plugin.py` 测试                      |
| `找不到可执行文件`                                                  | 先运行 `cargo build`；检查 `--binary` 的路径                                          |
| `plugin ID mismatch`                                                | 测试脚本中的 `--plugin-id` 必须与 `sdk::serve` 的 ID 一致                             |
| `initialize required`                                               | 宿主必须先发送 `plugin.initialize`，不能一开始直接调用自定义方法                      |
| `Unsupported method`                                                | 命令名拼错，或者 `match` 没处理这个命令                                               |
| `插件提前退出 / 无效协议头`                                         | 检查是否误用 `println!`，以及 `stderr` 日志                                           |
| 安装器找不到插件包                                                  | 必须在**公开仓库的 GitHub Release** 上传 `.fiexplugin` 资产，且文件名包含当前平台标识 |
| `plugin has no executable for this operating system / architecture` | manifest 的平台键不匹配，或压缩包根目录没有对应二进制                                 |
| `permission denied`                                                 | manifest 未声明对应宿主权限，或者用户尚未批准授权                                     |
| 安装同 ID 的新版本报错                                              | 目前需先卸载旧版本，再安装新版；注意保留需要的用户数据                                |

## 9. 安全须知，以及 SDK 目前不负责什么

- **所有插件都需要授权。** Fiex 官方与第三方插件没有权限特权，也不使用插件签名来免除授权。
- **Native 独立进程不等于操作系统沙箱。** 当前宿主 API 会检查授权，但原生插件仍可能直接访问操作系统允许当前用户访问的文件和网络。只运行信任来源的插件。
- SDK 处理 `Content-Length` + JSON-RPC 2.0 的 stdin/stdout 协议，当前协议版本 `2.0`，单帧最大 **4 MiB**；不要把大文件直接放入 JSON 返回值。
- 复杂插件的 UI、连接、远程文件系统、取消、流式传输和缓存/同步有各自的职责；**不要把 Fiex Core 的设计提案中的拟议代码当作当前已发布 SDK API。**

### 继续学习

- **最简单的真实 Rust 进程示例：** [mock_plugin.rs](examples/mock_plugin.rs)
- **可直接复制的官方功能插件：** [Checksum](https://github.com/kevinhuang001/fiex-official-plugins/tree/main/official/checksum)
- **有 iframe 界面的插件：** [Archive](https://github.com/kevinhuang001/fiex-official-plugins/tree/main/official/archive)
- **连接类插件：** [WebDAV](https://github.com/kevinhuang001/fiex-official-plugins/tree/main/official/webdav)
- **Fiex Core 插件技术文档：** [docs/plugins](https://github.com/kevinhuang001/fiex/tree/main/docs/plugins)（注意其中包含标注为“拟议”的设计，不都是当前 API）

**一句话总结：** 先学会 `sdk::serve` + `match` + JSON 返回值，跑通真实进程测试；再用 `manifest.json` 声明入口和权限，最后打包成 `.fiexplugin` 通过 GitHub Release 分发。

## Core-owned VFS views and connection eligibility

A standalone virtual tree declares `extensions.vfs: [{"id":"history","title":"Git history","readOnly":true}]`.
A connection factory declares `extensions.connections`; file browsing also requires `extensions.vfs`. Core assigns the mount ID and URI.
UI extensions never confer provider eligibility. `scheme`, `schemes`, `uriPrefix`, `mountId`, and `mountPath` are forbidden in these extensions.

Within a selected-resource request, call `ctx.open_vfs_view("history", selected_root, immutable_context)`.
Core authenticates the process identity, contribution ownership, and resource scope, and returns a `fiex://mount/<id>/` navigation URI.
The plugin must not construct URIs. Core sends contributionId/viewContext/nodeId with vfs.list/stat/readSmall/search; return opaque nodeId plus Entry metadata, and relativePath for search. Root nodeId is empty.
Core binds returned entries to its mount and retains only explicitly saved small view descriptors across restarts.

Connection-specific fs.* provider requests are Core-only and require an installed, active connection factory bound to that instance.
Ordinary VFS registration does not grant sync, pin, eviction, conflict, or cache control. Generic files/HTTP/TCP remain subject to their existing operation/network grants.
These are application navigation URIs, not OS mounts. Connection files stream to Core-managed cache files before external opening.
Standalone readSmall exports are bounded (1 MiB complete file); larger exports fail explicitly pending a streaming VFS contract.

### Request language

`ctx.locale().as_str()` returns `zh-CN`, `zh-TW` or `en`. Core supplies the
language in the JSON-RPC envelope; business `params` remain unchanged, including
arrays and an application-owned `params.locale` field. Each request captures its
own language, and SDK background tasks inherit it (`task.locale()`). The optional
`i18n::current_locale()` is scoped to the current SDK worker, with unwind-safe
restoration; unrelated requests and threads do not share mutable locale state.
