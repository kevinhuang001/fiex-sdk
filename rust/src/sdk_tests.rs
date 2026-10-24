//! Deterministic SDK tests through an in-memory host transport.
use crate::{
    context::{ContextInner, RequestContext, TaskContext},
    files::FileReader,
    http::Response,
    ipc::SharedBuffer,
    runtime::{session::Session, HostLink},
    tasks::{TaskManager, TaskStatus},
    tcp::TcpStream,
    RpcError,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

#[derive(Default)]
struct State {
    bytes: Vec<u8>,
    staged: Vec<u8>,
    position: usize,
    committed: bool,
    aborted: bool,
    read_closed: bool,
    http_closed: bool,
    tcp_closed: bool,
    http_position: usize,
    shared: Option<SharedBuffer>,
    shared_reads: usize,
    calls: Vec<String>,
    authorizations: Vec<Value>,
    tcp_input: Vec<u8>,
    tcp_output: Vec<u8>,
    task_started: Option<String>,
    task_finished: Option<(String, String)>,
    progress: Vec<(u64, Option<u64>)>,
    write_attempts: usize,
    shared_write_attempts: usize,
    inline_write_attempts: usize,
    fail_write: bool,
    fail_shared_write: bool,
    fail_buffer_create: bool,
}

struct Mock {
    state: Mutex<State>,
}

impl Mock {
    fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
        }
    }

    fn with_bytes(bytes: Vec<u8>) -> Self {
        Self {
            state: Mutex::new(State {
                bytes,
                ..State::default()
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    fn count(&self, method: &str) -> usize {
        self.state()
            .calls
            .iter()
            .filter(|call| call.as_str() == method)
            .count()
    }
}

impl HostLink for Mock {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let mut state = self.state.lock().unwrap();
        state.calls.push(method.to_string());
        state
            .authorizations
            .push(params.get("authorization").cloned().unwrap_or(Value::Null));
        if params.get("authorization").is_none() {
            return Err(RpcError::internal("missing authorization"));
        }
        match method {
            "host.fs.openRead" => Ok(json!({"handle":"read", "size":state.bytes.len()})),
            "host.fs.closeRead" => {
                state.read_closed = true;
                Ok(Value::Null)
            }
            "host.fs.readOpen" => {
                let start =
                    (params["offset"].as_u64().unwrap_or(0) as usize).min(state.bytes.len());
                let end = start
                    .saturating_add(params["length"].as_u64().unwrap_or(0) as usize)
                    .min(state.bytes.len());
                if params["buffer"].as_str().is_some() {
                    let source = state.bytes[start..end].to_vec();
                    state
                        .shared
                        .as_mut()
                        .ok_or_else(|| RpcError::internal("no shared buffer"))?
                        .fill(&source)
                        .map_err(RpcError::internal)?;
                    state.shared_reads += 1;
                    Ok(json!({"size":end-start,"eof":end==state.bytes.len()}))
                } else {
                    Ok(
                        json!({"base64":STANDARD.encode(&state.bytes[start..end]),"size":end-start,"eof":end==state.bytes.len()}),
                    )
                }
            }
            "host.buffer.create" => {
                if state.fail_buffer_create {
                    return Err(RpcError::internal("quota exceeded"));
                }
                let (buffer, descriptor) = SharedBuffer::create(
                    params["size"].as_u64().unwrap_or(1) as usize,
                    params["writable"].as_bool().unwrap_or(false),
                )
                .map_err(RpcError::internal)?;
                state.shared = Some(buffer);
                Ok(serde_json::to_value(descriptor).unwrap())
            }
            "host.buffer.release" => {
                state.shared = None;
                Ok(Value::Null)
            }
            "host.fs.beginWrite" => Ok(json!("write")),
            "host.fs.writeChunk" => {
                if state.fail_write {
                    state.write_attempts += 1;
                    return Err(RpcError::internal("connection lost"));
                }
                let bytes = STANDARD
                    .decode(params["base64"].as_str().unwrap_or(""))
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                write_staged(&mut state, &bytes);
                Ok(Value::Null)
            }
            "host.fs.writeBuffer" => {
                if state.fail_shared_write {
                    state.shared_write_attempts += 1;
                    return Err(RpcError::internal("connection lost"));
                }
                let size = params["size"].as_u64().unwrap_or(0) as usize;
                let bytes = state
                    .shared
                    .as_ref()
                    .ok_or_else(|| RpcError::internal("no shared buffer"))?
                    .as_slice()
                    .get(..size)
                    .ok_or_else(|| RpcError::internal("shared buffer too small"))?
                    .to_vec();
                write_staged(&mut state, &bytes);
                Ok(Value::Null)
            }
            "host.fs.seekWrite" => {
                state.position = params["offset"].as_i64().unwrap_or(0) as usize;
                Ok(json!(state.position))
            }
            "host.fs.commit" => {
                state.bytes = state.staged.clone();
                state.committed = true;
                Ok(Value::Null)
            }
            "host.fs.abort" => {
                state.aborted = true;
                Ok(Value::Null)
            }
            "host.fs.stat" => Ok(json!({
                "path":"file","name":"file","isDir":false,"size":state.bytes.len(),
                "modified":0,"extension":"txt","mime":"text/plain","readonly":false,"symlink":false
            })),
            "host.fs.list" => Ok(json!([])),
            "host.fs.mkdir" => Ok(Value::Null),
            "host.fs.remove" => Ok(Value::Null),
            "host.fs.rename" => Ok(Value::Null),
            "host.net.http.open" => {
                state.http_position = 0;
                Ok(
                    json!({"stream":"response","status":200,"headers":{"content-type":"application/octet-stream"}}),
                )
            }
            "host.net.http.read" => {
                let start = state.http_position.min(state.bytes.len());
                let end =
                    (start + params["size"].as_u64().unwrap_or(0) as usize).min(state.bytes.len());
                state.http_position = end;
                Ok(
                    json!({"base64":STANDARD.encode(&state.bytes[start..end]),"size":end-start,"eof":end==state.bytes.len()}),
                )
            }
            "host.net.http.close" => {
                state.http_closed = true;
                Ok(Value::Null)
            }
            "host.net.tcp.open" => {
                state.tcp_closed = false;
                Ok(json!({"stream":"tcp","peerAddress":"127.0.0.1:445"}))
            }
            "host.net.tcp.readBytes" => {
                let size = params["size"].as_u64().unwrap_or(0) as usize;
                let size = size.min(state.tcp_input.len());
                let bytes: Vec<u8> = state.tcp_input.drain(..size).collect();
                Ok(
                    json!({"base64":STANDARD.encode(&bytes),"size":bytes.len(),"eof":bytes.is_empty()}),
                )
            }
            "host.net.tcp.writeBytes" => {
                let bytes = STANDARD
                    .decode(params["base64"].as_str().unwrap_or(""))
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                state.tcp_output.extend_from_slice(&bytes);
                Ok(json!({"size":bytes.len()}))
            }
            "host.net.tcp.close" => {
                state.tcp_closed = true;
                Ok(Value::Null)
            }
            "host.task.start" => {
                state.task_started = params["taskId"].as_str().map(str::to_owned);
                Ok(Value::Null)
            }
            "host.task.finish" => {
                state.task_finished = Some((
                    params["taskId"].as_str().unwrap_or("").to_string(),
                    params["status"].as_str().unwrap_or("").to_string(),
                ));
                Ok(Value::Null)
            }
            "host.task.progress" => {
                state.progress.push((
                    params["done"].as_u64().unwrap_or(0),
                    params["total"].as_u64(),
                ));
                Ok(Value::Null)
            }
            "host.error.report" => Ok(Value::Null),
            other => Err(RpcError::method_not_found(other)),
        }
    }

    fn closed(&self) -> bool {
        false
    }
}

fn write_staged(state: &mut State, bytes: &[u8]) {
    if state.staged.len() < state.position + bytes.len() {
        state.staged.resize(state.position + bytes.len(), 0);
    }
    state.staged[state.position..state.position + bytes.len()].copy_from_slice(bytes);
    state.position += bytes.len();
}

fn context(link: Arc<Mock>) -> ContextInner {
    ContextInner::new(
        link,
        Arc::new(TaskManager::new(4)),
        Session::Operation(7),
        Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn friendly_files_stream_binary_data_seek_and_commit() {
    let bytes: Vec<u8> = (0..700_000).map(|n| (n % 256) as u8).collect();
    let mock = Arc::new(Mock::with_bytes(bytes.clone()));
    let inner = context(mock.clone());
    {
        let files = inner.files();
        assert_eq!(files.read("file").unwrap(), bytes);
        let mut reader = files.open_read("file").unwrap();
        reader.seek(SeekFrom::Start(255)).unwrap();
        let mut sample = [0; 3];
        reader.read_exact(&mut sample).unwrap();
        assert_eq!(sample, [255, 0, 1]);
        let mut writer = files.begin_write("file", false).unwrap();
        writer.write_all(b"first").unwrap();
        writer.seek(SeekFrom::Start(0)).unwrap();
        writer.write_all(b"new").unwrap();
        writer.commit().unwrap();
    }
    let state = mock.state();
    assert!(state.committed);
    assert!(!state.aborted);
    assert_eq!(state.bytes, b"newst");
}

#[test]
fn dropping_a_write_aborts_it() {
    let mock = Arc::new(Mock::new());
    {
        let inner = context(mock.clone());
        let files = inner.files();
        let mut writer = files.begin_write("file", true).unwrap();
        writer.write_all(b"uncommitted").unwrap();
    }
    let state = mock.state();
    assert!(state.aborted);
    assert!(!state.committed);
}

#[test]
fn http_preserves_binary_and_closes_response() {
    let bytes = vec![0, 255, 128, 10];
    let mock = Arc::new(Mock::with_bytes(bytes.clone()));
    let inner = context(mock.clone());
    {
        let http = inner.http();
        let response: Response = http.get("https://example.com/file").send().unwrap();
        assert_eq!(response.bytes().unwrap(), bytes);
    }
    assert!(mock.state().http_closed);
}

#[test]
fn http_json_is_a_regular_convenience_method() {
    let mock = Arc::new(Mock::with_bytes(br#"{"ok":true}"#.to_vec()));
    let inner = context(mock);
    let http = inner.http();
    let value: Value = http
        .post("https://example.com")
        .json(&json!({"request":true}))
        .unwrap()
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(value["ok"], true);
}

#[test]
fn small_reads_and_writes_are_batched_and_seek_uses_cached_bytes() {
    let mock = Arc::new(Mock::with_bytes(vec![42; 65_536]));
    let inner = context(mock.clone());
    {
        let files = inner.files();
        let mut reader = files.open_read("file").unwrap();
        for _ in 0..1000 {
            let mut byte = [0];
            reader.read_exact(&mut byte).unwrap();
            assert_eq!(byte, [42]);
        }
        reader.seek(SeekFrom::Start(3)).unwrap();
        let mut byte = [0];
        reader.read_exact(&mut byte).unwrap();
        let mut writer = files.begin_write("out", true).unwrap();
        for _ in 0..1000 {
            writer.write_all(b"x").unwrap();
        }
        writer.commit().unwrap();
    }
    assert_eq!(
        mock.count("host.fs.readOpen"),
        1,
        "small reads must not incur one IPC each"
    );
    assert_eq!(
        mock.count("host.fs.writeChunk"),
        1,
        "small writes must coalesce until commit"
    );
    assert_eq!(mock.state().bytes, vec![b'x'; 1000]);
}

#[test]
fn failed_flush_cannot_be_committed_or_retried_as_a_duplicate_write() {
    let mock = Arc::new(Mock::new());
    mock.state().fail_write = true;
    {
        let inner = context(mock.clone());
        let files = inner.files();
        let mut writer = files.begin_write("out", true).unwrap();
        writer.write_all(b"hello").unwrap();
        assert!(writer.flush().is_err());
        assert!(writer.commit().is_err());
    }
    let state = mock.state();
    assert_eq!(state.write_attempts, 1);
    assert!(state.aborted);
}

#[test]
fn large_read_reuses_shared_window_and_seek_reads_correct_bytes() {
    let mock = Arc::new(Mock::with_bytes((0..10_000_000).map(|n| n as u8).collect()));
    let inner = context(mock.clone());
    {
        let files = inner.files();
        let mut reader = files.open_read("file").unwrap();
        let mut part = vec![0; 2_000_000];
        reader.read_exact(&mut part).unwrap();
        assert_eq!(part[0], 0);
        assert_eq!(part[1_999_999], 1_999_999_u64 as u8);
        reader.seek(SeekFrom::Start(8_000_000)).unwrap();
        reader.read_exact(&mut part).unwrap();
        assert_eq!(part[1_999_999], 9_999_999_u64 as u8);
    }
    let state = mock.state();
    assert_eq!(state.shared_reads, 2);
    assert!(state.shared.is_none());
}

#[test]
fn large_write_reuses_shared_buffer_and_commits_once() {
    let mock = Arc::new(Mock::new());
    let bytes = vec![255; 3 * 256 * 1024 + 7];
    {
        let inner = context(mock.clone());
        inner.files().create("file", &bytes).unwrap();
    }
    let state = mock.state();
    assert_eq!(state.bytes, bytes);
    assert!(state.committed);
    assert!(state.shared.is_none());
    drop(state);
    assert_eq!(mock.count("host.fs.commit"), 1);
}

#[test]
fn shared_allocation_failure_uses_bounded_ipc() {
    let bytes = vec![0x80; 600_000];
    let mock = Arc::new(Mock::with_bytes(bytes.clone()));
    mock.state().fail_buffer_create = true;
    let inner = context(mock);
    let files = inner.files();
    let mut actual = Vec::new();
    files
        .open_read("file")
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, bytes);
}

#[test]
fn failed_shared_write_is_not_replayed_as_ipc() {
    let mock = Arc::new(Mock::new());
    mock.state().fail_shared_write = true;
    {
        let inner = context(mock.clone());
        let files = inner.files();
        let mut writer = files.begin_write("file", true).unwrap();
        writer.write_all(&vec![1; 256 * 1024]).unwrap();
        assert!(writer.commit().is_err());
    }
    let state = mock.state();
    assert_eq!(state.shared_write_attempts, 1);
    assert_eq!(state.inline_write_attempts, 0);
    assert!(state.aborted);
}

#[test]
fn reader_is_send_static_after_the_files_handle_is_dropped() {
    fn assert_send_static<T: Send + 'static>(_: &T) {}
    let mock = Arc::new(Mock::with_bytes(b"retained".to_vec()));
    let inner = context(mock.clone());
    let files = inner.files();
    let mut reader: FileReader = files.open_read("file").unwrap();
    assert_send_static(&reader);
    drop(files);
    drop(inner);
    let mut data = Vec::new();
    reader.read_to_end(&mut data).unwrap();
    assert_eq!(data, b"retained");
}

#[test]
fn tcp_stream_uses_standard_io_and_owns_its_connection() {
    fn assert_send_static<T: Send + 'static>(_: &T) {}
    let mock = Arc::new(Mock::new());
    mock.state().tcp_input = b"pong".to_vec();
    let inner = context(mock.clone());
    {
        let tcp = inner.tcp();
        let mut stream: TcpStream = tcp.connect("server", 445).unwrap();
        assert_send_static(&stream);
        stream.write_all(b"ping").unwrap();
        let mut response = [0; 4];
        stream.read_exact(&mut response).unwrap();
        assert_eq!(&response, b"pong");
        assert_eq!(stream.peer_addr().port(), 445);
    }
    let state = mock.state();
    assert_eq!(state.tcp_output, b"ping");
    assert!(state.tcp_closed);
}

#[test]
fn request_exposes_method_and_params() {
    let mock = Arc::new(Mock::new());
    let context = RequestContext::new(
        context(mock),
        "archive.compress".into(),
        json!({"format":"zip"}),
    );
    assert_eq!(context.method(), "archive.compress");
    assert_eq!(context.params()["format"], "zip");
}

#[test]
fn spawned_task_inherits_authorization_and_reports_completion() {
    let mock = Arc::new(Mock::with_bytes(b"task data".to_vec()));
    let context = RequestContext::new(context(mock.clone()), "task.test".into(), Value::Null);
    let handle = context
        .spawn_task(|task: TaskContext| {
            task.progress(1, Some(2));
            let data = task
                .files()
                .read_to_string("file")
                .map_err(|error| RpcError::internal(error.to_string()))?;
            assert_eq!(data, "task data");
            task.progress(2, Some(2));
            Ok(Value::Null)
        })
        .unwrap();
    handle.wait().unwrap();
    assert_eq!(handle.status(), TaskStatus::Completed);
    assert_eq!(handle.progress(), (2, Some(2)));
    let state = mock.state();
    assert!(state.task_started.is_some());
    assert_eq!(
        state
            .task_finished
            .as_ref()
            .map(|(_, status)| status.as_str()),
        Some("completed")
    );
    assert!(state
        .authorizations
        .iter()
        .any(|value| value.get("taskId").is_some()));
}

#[test]
fn task_cancellation_is_cooperative_and_reported() {
    let mock = Arc::new(Mock::new());
    let context = RequestContext::new(context(mock.clone()), "task.cancel".into(), Value::Null);
    let handle = context
        .spawn_task(|task: TaskContext| {
            while !task.is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(Value::Null)
        })
        .unwrap();
    handle.cancel();
    assert!(handle.wait().is_err());
    assert_eq!(handle.status(), TaskStatus::Cancelled);
    assert_eq!(
        mock.state()
            .task_finished
            .as_ref()
            .map(|(_, status)| status.as_str()),
        Some("cancelled")
    );
}

#[test]
fn task_limit_is_enforced() {
    let mock = Arc::new(Mock::new());
    let context = RequestContext::new(context(mock), "task.limit".into(), Value::Null);
    let mut handles = Vec::new();
    for _ in 0..4 {
        handles.push(
            context
                .spawn_task(|task: TaskContext| {
                    while !task.is_cancelled() {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Ok(Value::Null)
                })
                .unwrap(),
        );
    }
    assert!(context
        .spawn_task(|_: TaskContext| Ok(Value::Null))
        .is_err());
    for handle in &handles {
        handle.cancel();
    }
    for handle in handles {
        let _ = handle.wait();
    }
}
