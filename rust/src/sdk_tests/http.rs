use super::mock::{Mock, context};
use crate::http::Response;
use serde_json::{Value, json};
use std::sync::Arc;

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
fn streaming_body_writes_chunks_as_they_arrive_and_returns_the_response() {
    let mock = Arc::new(Mock::new());
    let inner = context(mock.clone());
    let mut body = inner
        .http()
        .put("https://example.com/upload")
        .header("content-type", "application/octet-stream")
        .body_stream(5)
        .unwrap();
    body.write_chunk(b"he").unwrap();
    body.write_chunk(b"llo").unwrap();
    let response = body.finish().unwrap();
    assert_eq!(response.status, 201);
    assert_eq!(response.headers["etag"], "v1");

    let state = mock.state();
    assert_eq!(state.http_open_params.len(), 1);
    let open = &state.http_open_params[0];
    assert_eq!(open["bodyStream"], true);
    assert_eq!(open["contentLength"], 5);
    // Streaming mode must not look like either buffered body mode.
    assert!(open.get("bodyBase64").is_none());
    assert!(open.get("bodyReadHandle").is_none());
    assert_eq!(
        state.http_body_chunks,
        vec![b"he".to_vec(), b"llo".to_vec()]
    );
    assert!(state.http_body_eof);
    assert_eq!(state.http_response_calls, 1);
}

#[test]
fn streaming_body_rejects_an_oversized_chunk_and_cancels() {
    let mock = Arc::new(Mock::new());
    let inner = context(mock.clone());
    let mut body = inner
        .http()
        .put("https://example.com/upload")
        .body_stream(3)
        .unwrap();
    let error = body.write_chunk(b"four").unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(mock.state().http_closed);
    // The body is aborted; nothing else can be written.
    assert!(body.write_chunk(b"a").is_err());
    drop(body);
}

#[test]
fn streaming_body_finish_requires_the_declared_length() {
    let mock = Arc::new(Mock::new());
    let inner = context(mock.clone());
    let mut body = inner
        .http()
        .put("https://example.com/upload")
        .body_stream(4)
        .unwrap();
    body.write_chunk(b"ab").unwrap();
    let error = match body.finish() {
        Ok(_) => panic!("short streaming body must not produce a response"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    // Core is told to cancel instead of waiting for its deadline.
    assert!(mock.state().http_closed);
}

#[test]
fn dropping_an_unfinished_streaming_body_cancels_the_request() {
    let mock = Arc::new(Mock::new());
    let inner = context(mock.clone());
    {
        let mut body = inner
            .http()
            .put("https://example.com/upload")
            .body_stream(2)
            .unwrap();
        body.write_chunk(b"a").unwrap();
    }
    assert!(mock.state().http_closed);
}

#[test]
fn a_failed_streaming_write_aborts_the_body() {
    let mock = Arc::new(Mock::new());
    mock.state().fail_http_body = true;
    let inner = context(mock.clone());
    let mut body = inner
        .http()
        .put("https://example.com/upload")
        .body_stream(2)
        .unwrap();
    assert!(body.write_chunk(b"a").is_err());
    assert!(mock.state().http_closed);
    assert!(body.write_chunk(b"b").is_err());
}
