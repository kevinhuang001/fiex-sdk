use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn main() {
    sdk::serve_with_host("org.fiex.mock", |method, value: Value, host| match method {
        "mock.echo" => Ok(json!({"echo": value})),
        "mock.directExec" => match std::process::Command::new("git").arg("--version").output() {
            Ok(_) => Err(RpcError::internal(
                "plugin process was allowed to execute git",
            )),
            Err(error) => Ok(json!({"blocked": true, "error": error.to_string()})),
        },
        "mock.directFile" => {
            let path = value
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            match std::fs::read(path) {
                Ok(_) => Err(RpcError::internal("plugin directly read a user file")),
                Err(error) => Ok(json!({"blocked":true,"error":error.to_string()})),
            }
        }
        "mock.directNet" => {
            let address = value
                .get("address")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing address"))?;
            match std::net::TcpStream::connect(address) {
                Ok(_) => Err(RpcError::internal("plugin opened a network socket")),
                Err(error) => Ok(json!({"blocked":true,"error":error.to_string()})),
            }
        }
        "mock.readFile" => {
            let path = value
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let files = host.files();
            Ok(json!(files
                .read_to_string(path)
                .map_err(|e| RpcError::internal(e.to_string()))?))
        }
        "mock.exec" => {
            let result = host.invoke(
                "host.exec",
                json!({"command": "git", "args": ["--version"]}),
            )?;
            Ok(json!({"stdout": result["stdout"]}))
        }
        "search.query" => Ok(json!({
            "entries": [{
                "path": "/tmp/fiex-mock-search.txt",
                "name": "fiex-mock-search.txt",
                "isDir": false,
                "isSymlink": false,
                "size": 12,
                "modified": 1,
                "created": 1,
                "extension": "txt",
                "mime": "text/plain",
                "readonly": false,
                "linkTarget": null,
                "tags": [],
                "sync": null
            }],
            "truncated": false
        })),
        _ => Err(RpcError::method_not_found(method)),
    })
    .expect("mock IPC process failed");
}
