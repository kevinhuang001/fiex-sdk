use fiex_plugin_sdk::{self as sdk, RpcError};
use serde_json::{json, Value};

fn main() {
    sdk::serve("org.fiex.mock", |method, value: Value| match method {
        "mock.echo" => Ok(json!({"echo": value})),
        _ => Err(RpcError::method_not_found(method)),
    })
    .expect("mock IPC process failed");
}
