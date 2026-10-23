use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn main() {
    sdk::serve_with_manifest_and_host(
        "org.fiex.mock",
        include_str!("../fixtures/mock-manifest.json"),
        |method, value: Value, host| match method {
            "mock.echo" => Ok(json!({"echo": value})),
            "mock.directExec" => {
                match std::process::Command::new("git").arg("--version").output() {
                    Ok(_) => Err(RpcError::internal(
                        "plugin process was allowed to execute git",
                    )),
                    Err(error) => Ok(json!({"blocked": true, "error": error.to_string()})),
                }
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
        },
    )
    .expect("mock IPC process failed");
}
