use base64::{engine::general_purpose::STANDARD, Engine};
use fiex_plugin_sdk::{HostServices, RpcError};
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom, Write};
#[derive(Default)]
struct Host {
    bytes: Vec<u8>,
    staged: Vec<u8>,
    position: usize,
    committed: bool,
    aborted: bool,
    closed: bool,
    shared: Option<fiex_plugin_sdk::ipc::SharedBuffer>,
    shared_reads: usize,
}
impl HostServices for Host {
    fn invoke(&mut self, method: &str, p: Value) -> Result<Value, RpcError> {
        match method {
            "host.fs.openRead" => Ok(json!({"handle":"read", "size":self.bytes.len()})),
            "host.fs.closeRead" => Ok(Value::Null),
            "host.fs.readOpen" => {
                let start = (p["offset"].as_u64().unwrap() as usize).min(self.bytes.len());
                let end = start
                    .saturating_add(p["length"].as_u64().unwrap() as usize)
                    .min(self.bytes.len());
                if p["buffer"].as_str().is_some() {
                    self.shared
                        .as_mut()
                        .unwrap()
                        .fill(&self.bytes[start..end])
                        .unwrap();
                    self.shared_reads += 1;
                    Ok(json!({"size":end-start,"eof":end==self.bytes.len()}))
                } else {
                    Ok(
                        json!({"base64":STANDARD.encode(&self.bytes[start..end]),"size":end-start,"eof":end==self.bytes.len()}),
                    )
                }
            }
            "host.buffer.create" => {
                let (buffer, descriptor) = fiex_plugin_sdk::ipc::SharedBuffer::create(
                    p["size"].as_u64().unwrap() as usize,
                    p["writable"].as_bool().unwrap_or(false),
                )
                .unwrap();
                self.shared = Some(buffer);
                Ok(serde_json::to_value(descriptor).unwrap())
            }
            "host.buffer.release" => {
                self.shared = None;
                Ok(Value::Null)
            }
            "host.fs.beginWrite" => Ok(json!("write")),
            "host.fs.writeChunk" => {
                let bytes = STANDARD.decode(p["base64"].as_str().unwrap()).unwrap();
                let end = self.position + bytes.len();
                self.staged.resize(end.max(self.staged.len()), 0);
                self.staged[self.position..end].copy_from_slice(&bytes);
                self.position = end;
                Ok(Value::Null)
            }
            "host.fs.writeBuffer" => {
                let size = p["size"].as_u64().unwrap() as usize;
                let bytes = &self.shared.as_ref().unwrap().as_slice()[..size];
                let end = self.position + size;
                self.staged.resize(end.max(self.staged.len()), 0);
                self.staged[self.position..end].copy_from_slice(bytes);
                self.position = end;
                Ok(Value::Null)
            }
            "host.fs.seekWrite" => {
                self.position = p["offset"].as_i64().unwrap() as usize;
                Ok(json!(self.position))
            }
            "host.fs.commit" => {
                self.bytes = self.staged.clone();
                self.committed = true;
                Ok(Value::Null)
            }
            "host.fs.abort" => {
                self.aborted = true;
                Ok(Value::Null)
            }
            "host.net.http.open" => Ok(
                json!({"stream":"response","status":200,"headers":{"content-type":"application/octet-stream"}}),
            ),
            "host.net.http.read" => {
                let start = self.position.min(self.bytes.len());
                let end = (start + p["size"].as_u64().unwrap() as usize).min(self.bytes.len());
                self.position = end;
                Ok(
                    json!({"base64":STANDARD.encode(&self.bytes[start..end]),"size":end-start,"eof":end==self.bytes.len()}),
                )
            }
            "host.net.http.close" => {
                self.closed = true;
                Ok(Value::Null)
            }
            _ => Err(RpcError::method_not_found(method)),
        }
    }
}
#[test]
fn friendly_files_stream_binary_data_seek_and_commit() {
    let bytes = (0..700000).map(|n| (n % 256) as u8).collect::<Vec<_>>();
    let mut host = Host {
        bytes: bytes.clone(),
        ..Default::default()
    };
    {
        let host: &mut dyn HostServices = &mut host;
        let files = host.files();
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
    assert!(host.committed);
    assert!(!host.aborted);
    assert_eq!(host.bytes, b"newst");
}
#[test]
fn dropping_a_write_aborts_it() {
    let mut host = Host::default();
    {
        let service: &mut dyn HostServices = &mut host;
        let files = service.files();
        let mut writer = files.begin_write("file", true).unwrap();
        writer.write_all(b"uncommitted").unwrap();
    }
    assert!(host.aborted);
    assert!(!host.committed);
}
#[test]
fn http_preserves_binary_and_closes_response() {
    let bytes = vec![0, 255, 128, 10];
    let mut host = Host {
        bytes: bytes.clone(),
        ..Default::default()
    };
    {
        let service: &mut dyn HostServices = &mut host;
        let http = service.http();
        assert_eq!(
            http.get("https://example.com/file")
                .send()
                .unwrap()
                .bytes()
                .unwrap(),
            bytes
        );
    }
    assert!(host.closed);
}
#[test]
fn http_json_is_a_regular_convenience_method() {
    let mut host = Host {
        bytes: br#"{"ok":true}"#.to_vec(),
        ..Default::default()
    };
    let service: &mut dyn HostServices = &mut host;
    let http = service.http();
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
    struct Counting {
        host: Host,
        reads: usize,
        writes: usize,
    }
    impl HostServices for Counting {
        fn invoke(&mut self, method: &str, p: Value) -> Result<Value, RpcError> {
            if method == "host.fs.readOpen" {
                self.reads += 1;
            }
            if method == "host.fs.writeChunk" {
                self.writes += 1;
            }
            self.host.invoke(method, p)
        }
    }
    let mut host = Counting {
        host: Host {
            bytes: vec![42; 65536],
            ..Default::default()
        },
        reads: 0,
        writes: 0,
    };
    {
        let service: &mut dyn HostServices = &mut host;
        let files = service.files();
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
    assert_eq!(host.reads, 1, "small reads must not incur one IPC each");
    assert_eq!(host.writes, 1, "small writes must coalesce until commit");
    assert_eq!(host.host.bytes, vec![b'x'; 1000]);
}

#[test]
fn failed_flush_cannot_be_committed_or_retried_as_a_duplicate_write() {
    struct Failing {
        attempts: usize,
        aborted: bool,
    }
    impl HostServices for Failing {
        fn invoke(&mut self, method: &str, _: Value) -> Result<Value, RpcError> {
            match method {
                "host.fs.beginWrite" => Ok(json!("write")),
                "host.fs.writeChunk" => {
                    self.attempts += 1;
                    Err(RpcError::internal("connection lost"))
                }
                "host.fs.abort" => {
                    self.aborted = true;
                    Ok(Value::Null)
                }
                _ => panic!("unexpected {method}"),
            }
        }
    }
    let mut host = Failing {
        attempts: 0,
        aborted: false,
    };
    {
        let service: &mut dyn HostServices = &mut host;
        let files = service.files();
        let mut writer = files.begin_write("out", true).unwrap();
        writer.write_all(b"hello").unwrap();
        assert!(writer.flush().is_err());
        assert!(writer.commit().is_err());
    }
    assert_eq!(host.attempts, 1);
    assert!(host.aborted);
}

#[test]
fn large_read_reuses_shared_window_and_seek_reads_correct_bytes() {
    let mut host = Host {
        bytes: (0..10_000_000).map(|n| n as u8).collect(),
        ..Default::default()
    };
    {
        let service: &mut dyn HostServices = &mut host;
        let files = service.files();
        let mut reader = files.open_read("file").unwrap();
        let mut part = vec![0; 2_000_000];
        reader.read_exact(&mut part).unwrap();
        assert_eq!(part[0], 0);
        assert_eq!(part[1_999_999], 1_999_999_u64 as u8);
        reader.seek(SeekFrom::Start(8_000_000)).unwrap();
        reader.read_exact(&mut part).unwrap();
        assert_eq!(part[1_999_999], 9_999_999_u64 as u8);
    }
    assert_eq!(host.shared_reads, 2);
    assert!(host.shared.is_none());
}

#[test]
fn large_write_reuses_shared_buffer_and_commits_once() {
    let mut host = Host::default();
    let bytes = vec![255; 3 * 256 * 1024 + 7];
    {
        let service: &mut dyn HostServices = &mut host;
        let files = service.files();
        files.create("file", &bytes).unwrap();
    }
    assert_eq!(host.bytes, bytes);
    assert!(host.committed);
    assert!(host.shared.is_none());
}

#[test]
fn shared_allocation_failure_uses_bounded_ipc() {
    struct NoShared(Host);
    impl HostServices for NoShared {
        fn invoke(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
            if method == "host.buffer.create" {
                return Err(RpcError::internal("quota exceeded"));
            }
            self.0.invoke(method, params)
        }
    }
    let bytes = vec![0x80; 600_000];
    let mut host = NoShared(Host {
        bytes: bytes.clone(),
        ..Default::default()
    });
    let service: &mut dyn HostServices = &mut host;
    let files = service.files();
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
    struct Reject {
        host: Host,
        shared_attempts: usize,
        inline_attempts: usize,
    }
    impl HostServices for Reject {
        fn invoke(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
            if method == "host.fs.writeBuffer" {
                self.shared_attempts += 1;
                return Err(RpcError::internal("connection lost"));
            }
            if method == "host.fs.writeChunk" {
                self.inline_attempts += 1;
            }
            self.host.invoke(method, params)
        }
    }
    let mut host = Reject {
        host: Host::default(),
        shared_attempts: 0,
        inline_attempts: 0,
    };
    {
        let service: &mut dyn HostServices = &mut host;
        let files = service.files();
        let mut writer = files.begin_write("file", true).unwrap();
        writer.write_all(&vec![1; 256 * 1024]).unwrap();
        assert!(writer.commit().is_err());
    }
    assert_eq!(host.shared_attempts, 1);
    assert_eq!(host.inline_attempts, 0);
    assert!(host.host.aborted);
}
