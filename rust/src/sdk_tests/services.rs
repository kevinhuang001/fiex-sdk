use super::mock::{Mock, context, reference};
use crate::context::RequestContext;
use serde_json::json;
use std::{
    io::{Read, Seek, SeekFrom},
    sync::Arc,
};

#[test]
fn browser_oauth_uses_the_authenticated_host_service() {
    let mock = Arc::new(Mock::new());
    let ctx = RequestContext::new(
        context(mock.clone()),
        "connection.validate".into(),
        json!({}),
    );
    let request = crate::OAuthRequest::new(
        "https://accounts.google.com/o/oauth2/v2/auth",
        "user-owned-client",
        vec!["drive".into()],
    );
    let code = ctx.oauth().authorize(&request).unwrap();
    assert_eq!(code.code, "one-use");
    assert_eq!(code.code_verifier, "verifier");
    let state = mock.state.lock().unwrap();
    assert_eq!(state.calls, vec!["host.oauth.authorize"]);
    assert!(state.authorizations.iter().all(|value| !value.is_null()));
}

#[test]
fn file_reader_rejects_truncation_instead_of_hashing_partial_content() {
    let mock = Arc::new(Mock::with_bytes(b"original".to_vec()));
    let inner = context(mock.clone());
    let mut reader = inner.files().open_read(&reference()).unwrap();
    mock.state().bytes = b"cut".to_vec();
    let mut output = Vec::new();
    let error = reader.read_to_end(&mut output).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
}

#[test]
fn file_reader_obeys_snapshot_length_and_seek_past_eof() {
    let mock = Arc::new(Mock::with_bytes(b"old".to_vec()));
    let inner = context(mock.clone());
    let mut reader = inner.files().open_read(&reference()).unwrap();
    mock.state().bytes = b"old plus newer data".to_vec();
    let mut output = Vec::new();
    reader.read_to_end(&mut output).unwrap();
    assert_eq!(output, b"old");
    reader.seek(SeekFrom::Start(100)).unwrap();
    assert_eq!(reader.read(&mut [0; 8]).unwrap(), 0);
}
