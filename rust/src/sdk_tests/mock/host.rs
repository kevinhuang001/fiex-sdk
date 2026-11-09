use super::{write_staged, Mock};
use crate::{ipc::SharedBuffer, runtime::HostLink, RpcError};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};

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
        if method.starts_with("host.net.tcp.") {
            state.tcp_deadlines.push(params["deadlineMs"].clone());
        }
        match method {
            "host.oauth.authorize" => {
                assert_eq!(params["clientId"], "user-owned-client");
                assert_eq!(
                    params["authorizationEndpoint"],
                    "https://accounts.google.com/o/oauth2/v2/auth"
                );
                Ok(
                    json!({"code":"one-use","redirectUri":"http://127.0.0.1:1234/oauth/callback","codeVerifier":"verifier"}),
                )
            }
            "host.storage.get" => {
                let value = state.storage.get(params["key"].as_str().unwrap()).cloned();
                Ok(json!({"found":value.is_some(),"value":value}))
            }
            "host.storage.set" => {
                state.storage.insert(
                    params["key"].as_str().unwrap().into(),
                    params["value"].clone(),
                );
                Ok(Value::Null)
            }
            "host.storage.delete" => {
                state.storage.remove(params["key"].as_str().unwrap());
                Ok(Value::Null)
            }
            "host.storage.keys" => Ok(json!(state.storage.keys().collect::<Vec<_>>())),
            "host.storage.compareAndSet" => {
                let key = params["key"].as_str().unwrap();
                let expected = if params["expectedExists"] == true {
                    Some(&params["expected"])
                } else {
                    None
                };
                let matched = state.storage.get(key) == expected;
                if matched {
                    if params["delete"] == true {
                        state.storage.remove(key);
                    } else {
                        state.storage.insert(key.into(), params["value"].clone());
                    }
                }
                Ok(json!(matched))
            }
            "host.secret.available" => Ok(json!(true)),
            "host.secret.get" => {
                if let Some(status) = &state.secret_status {
                    return Ok(json!({"status":status}));
                }
                Ok(match &state.secret {
                    Some(value) => json!({"status":"present","value":value}),
                    None => json!({"status":"missing"}),
                })
            }
            "host.secret.set" => {
                state.secret = params["value"].as_str().map(str::to_owned);
                Ok(json!({"status":"ok"}))
            }
            "host.secret.delete" => {
                state.secret = None;
                Ok(json!({"status":"ok"}))
            }
            "host.fs.openRead" => Ok(json!({"handle":"read", "size":state.bytes.len()})),
            "host.fs.resolveMapped" => Ok(json!({"ref": {
                "mountId":"mapped-mount", "nodeId":"mapped-node"
            }})),
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
                "modified":0,"extension":"txt","mime":"text/plain","symlink":false
            })),
            "host.fs.list" => Ok(json!([])),
            "host.fs.mkdir" => Ok(Value::Null),
            "host.fs.remove" => Ok(Value::Null),
            "host.fs.rename" => Ok(Value::Null),
            "host.net.http.open" => {
                state.http_position = 0;
                state.http_open_params.push(params.clone());
                if params["bodyStream"] == true {
                    Ok(json!({"handle":"upload"}))
                } else {
                    Ok(
                        json!({"stream":"response","status":200,"headers":{"content-type":"application/octet-stream"}}),
                    )
                }
            }
            "host.net.http.body" => {
                if state.fail_http_body {
                    return Err(RpcError::internal("connection lost"));
                }
                if params["handle"] != "upload" {
                    return Err(RpcError::internal("unknown streaming body"));
                }
                let bytes = STANDARD
                    .decode(params["base64"].as_str().unwrap_or(""))
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                state.http_body_written += bytes.len();
                if !bytes.is_empty() {
                    state.http_body_chunks.push(bytes);
                }
                if params["eof"] == true {
                    state.http_body_eof = true;
                }
                Ok(json!({"written":state.http_body_written,"eof":params["eof"] == true}))
            }
            "host.net.http.response" => {
                state.http_response_calls += 1;
                Ok(json!({"stream":"upload","status":201,"headers":{"etag":"v1"}}))
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
            "host.error.report" | "host.log.report" => {
                state.diagnostics.push((method.into(), params));
                Ok(Value::Null)
            }
            other => Err(RpcError::method_not_found(other)),
        }
    }

    fn closed(&self) -> bool {
        false
    }
}
