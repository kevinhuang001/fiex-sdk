use super::*;
use std::sync::Mutex;

#[path = "streams/basic.rs"]
mod basic;

/// Host mock that serves one HTTP response and remembers every
/// authorization it was called with.
struct StreamHost {
    bytes: Vec<u8>,
    position: Mutex<usize>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl StreamHost {
    fn new(bytes: &[u8]) -> Arc<Self> {
        Arc::new(Self {
            bytes: bytes.to_vec(),
            position: Mutex::new(0),
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<(String, Value)> {
        self.calls.lock().unwrap().clone()
    }

    fn authorization(&self, method: &str) -> Value {
        self.calls()
            .into_iter()
            .find(|(call, _)| call == method)
            .map(|(_, params)| params["authorization"].clone())
            .unwrap_or(Value::Null)
    }
}

impl HostLink for StreamHost {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.calls
            .lock()
            .unwrap()
            .push((method.to_string(), params.clone()));
        match method {
            "host.task.start" => Ok(Value::Null),
            "host.task.finish" => Ok(Value::Null),
            "host.net.http.open" => Ok(json!({"stream":"s","status":200,"headers":{}})),
            "host.net.http.read" => {
                let size = params["size"].as_u64().unwrap_or(0) as usize;
                let mut position = self.position.lock().unwrap();
                let end = (*position + size).min(self.bytes.len());
                let chunk = &self.bytes[*position..end];
                *position = end;
                use base64::{Engine, engine::general_purpose::STANDARD};
                Ok(json!({"base64": STANDARD.encode(chunk), "size": chunk.len()}))
            }
            "host.net.http.close" => Ok(Value::Null),
            other => Err(RpcError::method_not_found(other)),
        }
    }

    fn closed(&self) -> bool {
        false
    }
}

/// Streams a host HTTP response, the way WebDAV/Drive/OpenList do.
struct Http;

impl VfsProvider for Http {
    fn operations(&self) -> &'static [crate::filesystem::FsOperation] {
        &[crate::filesystem::FsOperation::Read]
    }

    fn read(&self, request: &FsRequest<'_>) -> Result<FsRead, RpcError> {
        let response = request
            .ctx()
            .http()
            .get("https://example.com/file")
            .send()
            .map_err(|error| RpcError::internal(error.to_string()))?;
        Ok(FsRead::new(response, None))
    }
}

fn session_request(link: Arc<StreamHost>, method: &str, params: Value) -> RequestContext {
    RequestContext::new(
        ContextInner::new(
            link,
            Arc::new(TaskManager::new(4)),
            Session::Operation(7),
            Arc::new(AtomicBool::new(false)),
        ),
        method.into(),
        params,
    )
}

#[test]
fn http_backed_streams_keep_their_handles_across_requests() {
    let host = StreamHost::new(b"abcdef");
    let server = FsServer::new();
    let opened = server
        .serve(
            &Http,
            &session_request(host.clone(), "vfs.openRead", json!({"nodeId": "f"})),
        )
        .unwrap()
        .unwrap();
    let id = opened["streamId"].as_str().unwrap().to_string();
    // Default chunking splits the body, so the second chunk is read by a
    // later request exactly like Core does.
    let first = server
        .serve(
            &Http,
            &session_request(
                host.clone(),
                "vfs.readStream",
                json!({"streamId": id, "offset": 0, "maxBytes": 3}),
            ),
        )
        .unwrap()
        .unwrap();
    let second = server
        .serve(
            &Http,
            &session_request(
                host.clone(),
                "vfs.readStream",
                json!({"streamId": id, "offset": 3, "maxBytes": 3}),
            ),
        )
        .unwrap()
        .unwrap();
    assert_eq!(first["base64"], "YWJj");
    assert_eq!(second["base64"], "ZGVm");

    // The provider's handle calls run under the stream's own task, not the
    // already-finished `vfs.openRead` request.
    let open = host.authorization("host.net.http.open");
    let read = host.authorization("host.net.http.read");
    let task = open["taskId"].as_str().expect("stream task id");
    assert_eq!(read["taskId"], task);
    assert!(open.get("operationId").is_none());

    server
        .serve(
            &Http,
            &session_request(host.clone(), "vfs.closeRead", json!({"streamId": id})),
        )
        .unwrap()
        .unwrap();
    let finish = host
        .calls()
        .into_iter()
        .find(|(call, _)| call == "host.task.finish")
        .expect("stream task finished");
    assert_eq!(finish.1["taskId"], task);
    assert_eq!(finish.1["status"], "completed");
}

fn scoped_stream_call(
    server: &FsServer,
    provider: &Memory,
    method: &str,
    mount: &str,
    id: &str,
) -> Result<Option<Value>, RpcError> {
    let params = json!({
        "mountId": mount, "streamId": id, "offset": 0,
        "maxBytes": 2, "base64": "YQ=="
    });
    server.serve(provider, &request(method, params))
}

#[test]
fn read_and_write_sessions_are_restricted_to_the_opening_mount() {
    let server = FsServer::new();
    let provider = Memory { writable: true };
    let open_read = request("vfs.openRead", json!({"mountId":"account-a","nodeId":"a"}));
    let opened = server.serve(&provider, &open_read).unwrap().unwrap();
    let read_id = opened["streamId"].as_str().unwrap();
    for method in ["vfs.readStream", "vfs.closeRead"] {
        assert!(scoped_stream_call(&server, &provider, method, "account-b", read_id).is_err());
    }
    assert!(scoped_stream_call(&server, &provider, "vfs.commit", "account-a", read_id).is_err());
    assert!(scoped_stream_call(&server, &provider, "vfs.abort", "account-a", read_id).is_err());
    let chunk = scoped_stream_call(&server, &provider, "vfs.readStream", "account-a", read_id)
        .unwrap()
        .unwrap();
    assert_eq!(chunk["base64"], "YWI=");
    scoped_stream_call(&server, &provider, "vfs.closeRead", "account-a", read_id).unwrap();

    let open_write = request(
        "vfs.beginWrite",
        json!({"mountId":"account-a","nodeId":"a"}),
    );
    let opened = server.serve(&provider, &open_write).unwrap().unwrap();
    let write_id = opened["streamId"].as_str().unwrap();
    for method in ["vfs.writeStream", "vfs.commit", "vfs.abort"] {
        assert!(scoped_stream_call(&server, &provider, method, "account-b", write_id).is_err());
    }
    assert!(
        scoped_stream_call(&server, &provider, "vfs.closeRead", "account-a", write_id).is_err()
    );
    scoped_stream_call(&server, &provider, "vfs.writeStream", "account-a", write_id).unwrap();
    scoped_stream_call(&server, &provider, "vfs.commit", "account-a", write_id).unwrap();
}
