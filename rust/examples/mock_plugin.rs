use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn main() {
    sdk::serve("org.fiex.mock", |method, value: Value| match method {
        "mock.echo" => Ok(json!({"echo": value})),
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
