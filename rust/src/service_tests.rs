use super::*;
#[test]
fn storage_typed_update_is_atomic_across_concurrent_tasks_and_distinguishes_null() {
    let mock = Arc::new(Mock::new());
    let storage = crate::Storage::new(context(mock));
    storage.set("null", &Value::Null).unwrap();
    assert_eq!(storage.get::<Value>("null").unwrap(), Some(Value::Null));
    assert!(!storage
        .compare_and_set::<Value>("null", None, Some(&json!(1)))
        .unwrap());
    let workers = (0..4)
        .map(|_| {
            let storage = storage.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    storage
                        .update("count", |value: Option<u64>| Some(value.unwrap_or(0) + 1))
                        .unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(storage.get::<u64>("count").unwrap(), Some(80));
    storage.delete("count").unwrap();
    assert_eq!(storage.get::<u64>("count").unwrap(), None);
}
#[test]
fn secrets_have_distinct_missing_denied_and_unavailable_states_and_typed_roundtrips() {
    let mock = Arc::new(Mock::new());
    let secrets = crate::Secrets::new(context(mock.clone()));
    assert_eq!(secrets.get::<String>("password").unwrap(), None);
    secrets.set("password", "fiex-json:v1:literal").unwrap();
    assert_eq!(
        secrets.get::<String>("password").unwrap().as_deref(),
        Some("fiex-json:v1:literal")
    );
    secrets
        .set("typed", &json!({"token":"private","expires":3}))
        .unwrap();
    assert_eq!(
        secrets.get::<Value>("typed").unwrap(),
        Some(json!({"token":"private","expires":3}))
    );
    mock.state().secret_status = Some("denied".into());
    assert!(matches!(
        secrets.get::<String>("password"),
        Err(crate::SecretError::Denied(_))
    ));
    mock.state().secret_status = Some("unavailable".into());
    assert!(matches!(
        secrets.get::<String>("password"),
        Err(crate::SecretError::Unavailable(_))
    ));
}
#[test]
fn tcp_uses_one_deadline_for_connect_read_and_write_and_rejects_expired_before_ipc() {
    let mock = Arc::new(Mock::new());
    mock.state().tcp_input = b"a".to_vec();
    let client = context(mock.clone())
        .tcp()
        .with_deadline(std::time::Instant::now() + Duration::from_secs(2));
    let mut stream = client.connect("example.com", 445).unwrap();
    stream.read_exact(&mut [0u8; 1]).unwrap();
    stream.write_all(b"b").unwrap();
    let state = mock.state();
    let deadlines = &state.tcp_deadlines;
    assert_eq!(deadlines.len(), 3);
    assert!(deadlines[0].is_number());
    assert!(deadlines.iter().all(|deadline| deadline == &deadlines[0]));
    drop(state);
    let count = mock.count("host.net.tcp.open");
    assert_eq!(
        context(mock.clone())
            .tcp()
            .with_deadline(std::time::Instant::now())
            .connect("example.com", 445)
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    assert_eq!(mock.count("host.net.tcp.open"), count);
}
#[cfg(feature = "tokio")]
#[test]
fn tokio_adapter_preserves_cancelled_read_bytes_and_flushes_owned_writes_through_broker() {
    use std::{
        pin::Pin,
        task::{Context, Poll, Waker},
    };
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};
    let mock = Arc::new(Mock::new());
    mock.state().tcp_input = b"abc".to_vec();
    let mut stream = context(mock.clone())
        .tcp()
        .connect("example.com", 445)
        .unwrap()
        .into_async();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    {
        let _entered = runtime.enter();
        let guard = mock.state(); // Hold the broker to make the first poll deterministically Pending.
        let mut bytes = [0u8; 3];
        let mut out = ReadBuf::new(&mut bytes);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            Pin::new(&mut stream).poll_read(&mut cx, &mut out),
            Poll::Pending
        ));
        drop(guard); // Cancel the read future and use a smaller buffer next time.
    }
    runtime.block_on(async {
        let mut first = [0u8; 1];
        stream.read_exact(&mut first).await.unwrap();
        assert_eq!(&first, b"a");
        let mut rest = [0u8; 2];
        stream.read_exact(&mut rest).await.unwrap();
        assert_eq!(&rest, b"bc");
        stream.write_all(b"owned").await.unwrap();
        stream.flush().await.unwrap();
    });
    assert_eq!(mock.state().tcp_output, b"owned");
    assert_eq!(mock.count("host.net.tcp.readBytes"), 1);
}
