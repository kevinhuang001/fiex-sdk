#!/usr/bin/env python3
"""用真实可执行文件检查 Fiex Native 插件的初始化、调用与退出。

仅依赖 Python 3 标准库；可以在 Windows / macOS / Linux 上运行。
示例：
  python sdk/rust/examples/check_plugin.py --binary target/debug/fiex-plugin-hello --plugin-id org.example.hello --method hello.greet
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def send(proc, request):
    body = json.dumps(request, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    proc.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body)
    proc.stdin.flush()


def receive(proc):
    size = None
    while True:
        line = proc.stdout.readline()
        if not line:
            raise RuntimeError("插件提前退出或向 stdout 输出了非协议内容")
        if line == b"\r\n":
            break
        key, separator, value = line.partition(b":")
        if not separator:
            raise RuntimeError(f"无效的协议头：{line!r}")
        if key.lower() == b"content-length":
            size = int(value.strip())
            if not 0 <= size <= 4 * 1024 * 1024:
                raise RuntimeError("Content-Length 超出范围")
    if size is None:
        raise RuntimeError("响应缺少 Content-Length")
    data = proc.stdout.read(size)
    if len(data) != size:
        raise RuntimeError("响应 JSON 内容不完整")
    return json.loads(data)


def request(proc, request_id, method, params):
    send(proc, {
        "jsonrpc": "2.0",
        "id": request_id,
        "method": method,
        "params": params,
    })
    response = receive(proc)
    if response.get("id") != request_id:
        raise RuntimeError(f"响应 ID 不匹配：{response!r}")
    return response


def main():
    # GitHub Actions on Windows uses a legacy console encoding by default.
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except AttributeError:
        pass
    parser = argparse.ArgumentParser(description="Fiex 原生插件的最简进程测试")
    parser.add_argument("--binary", required=True, help="cargo build 生成的可执行文件路径")
    parser.add_argument("--plugin-id", required=True, help="应与 sdk::serve 的 ID 一致")
    parser.add_argument("--method", required=True, help="要调用的方法，例如 hello.greet")
    parser.add_argument("--params", default='{"name":"新手"}', help="JSON 参数字符串")
    args = parser.parse_args()

    binary = Path(args.binary).expanduser()
    if os.name == "nt" and binary.suffix.lower() != ".exe":
        binary = binary.with_suffix(".exe")
    binary = binary.resolve()
    if not binary.is_file():
        parser.error(f"找不到可执行文件：{binary}。先运行 cargo build。")
    try:
        params = json.loads(args.params)
    except json.JSONDecodeError as exc:
        parser.error(f"--params 不是有效 JSON：{exc}")

    with subprocess.Popen(
        [str(binary)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ) as proc:
        try:
            initialized = request(
                proc, 1, "plugin.initialize", {"protocolVersion": "3.0"}
            )
            if initialized.get("result", {}).get("pluginId") != args.plugin_id:
                raise RuntimeError(f"插件初始化失败或 ID 不匹配：{initialized!r}")
            print("[OK] 初始化成功：", initialized["result"])

            answer = request(proc, 2, args.method, params)
            if "error" in answer:
                raise RuntimeError(f"方法调用失败：{answer['error']!r}")
            print("[OK] 方法调用成功：", json.dumps(
                answer["result"], ensure_ascii=False
            ))

            stopped = request(proc, 3, "plugin.shutdown", {})
            if "error" in stopped:
                raise RuntimeError(f"关闭失败：{stopped['error']!r}")
            proc.stdin.close()
            if proc.wait(timeout=10) != 0:
                raise RuntimeError(f"插件退出码为 {proc.returncode}")
            print("[OK] 正常退出，三项检查全部通过")
        except Exception as exc:
            print(f"[FAIL] 测试失败：{exc}", file=sys.stderr)
            if proc.poll() is None:
                proc.kill()
                proc.wait()
            stderr = proc.stderr.read().decode("utf-8", errors="replace").strip()
            if stderr:
                print("插件 stderr 日志：\n" + stderr, file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
