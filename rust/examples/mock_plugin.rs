use fiex_plugin_sdk::{self as sdk, Files, RpcError};
use serde_json::{json, Value};
use std::sync::Mutex;

static LATE_FILES: Mutex<Option<Files>> = Mutex::new(None);

fn main() {
    sdk::serve("org.fiex.mock", |ctx| match ctx.method() {
        "mock.echo" => Ok(json!({"echo": ctx.params()})),
        "mock.directExec" => match std::process::Command::new("git").arg("--version").output() {
            Ok(_) => Err(RpcError::internal(
                "plugin process was allowed to execute git",
            )),
            Err(error) => Ok(json!({"blocked": true, "error": error.to_string()})),
        },
        "mock.directFile" => {
            let path = ctx
                .params()
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            match std::fs::read(path) {
                Ok(_) => Err(RpcError::internal("plugin directly read a user file")),
                Err(error) => Ok(json!({"blocked":true,"error":error.to_string()})),
            }
        }
        "mock.directNet" => {
            let address = ctx
                .params()
                .get("address")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing address"))?;
            match std::net::TcpStream::connect(address) {
                Ok(_) => Err(RpcError::internal("plugin opened a network socket")),
                Err(error) => Ok(json!({"blocked":true,"error":error.to_string()})),
            }
        }
        "mock.readFile" => {
            let path = ctx
                .params()
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let files = ctx.files();
            Ok(json!(files
                .read_to_string(path)
                .map_err(|error| RpcError::internal(error.to_string()))?))
        }
        "mock.spawnTask" => {
            let path = ctx
                .params()
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?
                .to_owned();
            let task = ctx.spawn_task(move |task| {
                std::thread::sleep(std::time::Duration::from_millis(150));
                let text = task
                    .files()
                    .read_to_string(&path)
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                task.progress(1, Some(1));
                Ok(json!(text))
            })?;
            Ok(json!({"taskId": task.id()}))
        }
        "mock.captureTaskScope" => {
            let task = ctx.spawn_task(|task| {
                *LATE_FILES
                    .lock()
                    .map_err(|_| RpcError::internal("scope lock"))? = Some(task.files());
                Ok(Value::Null)
            })?;
            Ok(json!({"taskId": task.id()}))
        }
        "mock.useFinishedTaskScope" => {
            let files = LATE_FILES
                .lock()
                .map_err(|_| RpcError::internal("scope lock"))?
                .take()
                .ok_or_else(|| RpcError::internal("no captured task scope"))?;
            files
                .read_to_string("secret.txt")
                .map_err(|error| RpcError::internal(error.to_string()))?;
            Err(RpcError::internal("stale task scope was still authorized"))
        }
        "mock.longTask" => {
            let task = ctx.spawn_task(|task| {
                while !task.is_cancelled() {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Ok(Value::Null)
            })?;
            Ok(json!({"taskId": task.id()}))
        }
        "mock.rendezvous" => {
            use std::sync::{Barrier, OnceLock};
            static BARRIER: OnceLock<Barrier> = OnceLock::new();
            BARRIER.get_or_init(|| Barrier::new(2)).wait();
            Ok(json!({"met": true}))
        }
        "mock.exec" => {
            let result = ctx.invoke(
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
        other => Err(RpcError::method_not_found(other)),
    })
    .expect("mock IPC process failed");
}
