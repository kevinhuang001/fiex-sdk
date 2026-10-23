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
}
impl HostServices for Host {
    fn invoke(&mut self, method: &str, p: Value) -> Result<Value, RpcError> {
        match method {
            "host.fs.stat" => Ok(
                json!({"path":"file","name":"file","isDir":false,"size":self.bytes.len(),"modified":0,"extension":"","mime":"application/octet-stream","readonly":false,"symlink":false}),
            ),
            "host.fs.readChunk" => {
                let start = (p["offset"].as_u64().unwrap() as usize).min(self.bytes.len());
                let end = start
                    .saturating_add(p["length"].as_u64().unwrap() as usize)
                    .min(self.bytes.len());
                Ok(
                    json!({"base64":STANDARD.encode(&self.bytes[start..end]),"size":end-start,"eof":end==self.bytes.len()}),
                )
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
