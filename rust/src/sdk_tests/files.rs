use super::mock::{context, reference, Mock};
use crate::FileReader;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    sync::Arc,
};

#[test]
fn friendly_files_stream_binary_data_seek_and_commit() {
    let bytes: Vec<u8> = (0..700_000).map(|n| (n % 256) as u8).collect();
    let mock = Arc::new(Mock::with_bytes(bytes.clone()));
    let inner = context(mock.clone());
    {
        let files = inner.files();
        assert_eq!(files.read(&reference()).unwrap(), bytes);
        let mut reader = files.open_read(&reference()).unwrap();
        reader.seek(SeekFrom::Start(255)).unwrap();
        let mut sample = [0; 3];
        reader.read_exact(&mut sample).unwrap();
        assert_eq!(sample, [255, 0, 1]);
        let mut writer = files.begin_write(&reference(), false).unwrap();
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
fn mapped_file_resolution_uses_the_scoped_host_operation() {
    let mock = Arc::new(Mock::new());
    let reference = context(mock.clone())
        .files()
        .resolve_mapped(
            "plugin://provider/source",
            crate::filesystem::FsOperation::Read,
        )
        .unwrap();
    assert_eq!(reference.mount_id, "mapped-mount");
    assert_eq!(reference.node_id, "mapped-node");
    let state = mock.state();
    assert_eq!(state.calls, vec!["host.fs.resolveMapped"]);
    assert!(state
        .authorizations
        .iter()
        .all(|authorization| !authorization.is_null()));
}

#[test]
fn dropping_a_write_aborts_it() {
    let mock = Arc::new(Mock::new());
    {
        let inner = context(mock.clone());
        let files = inner.files();
        let mut writer = files.begin_write(&reference(), true).unwrap();
        writer.write_all(b"uncommitted").unwrap();
    }
    let state = mock.state();
    assert!(state.aborted);
    assert!(!state.committed);
}

#[test]
fn small_reads_and_writes_are_batched_and_seek_uses_cached_bytes() {
    let mock = Arc::new(Mock::with_bytes(vec![42; 65_536]));
    let inner = context(mock.clone());
    {
        let files = inner.files();
        let mut reader = files.open_read(&reference()).unwrap();
        for _ in 0..1000 {
            let mut byte = [0];
            reader.read_exact(&mut byte).unwrap();
            assert_eq!(byte, [42]);
        }
        reader.seek(SeekFrom::Start(3)).unwrap();
        let mut byte = [0];
        reader.read_exact(&mut byte).unwrap();
        let mut writer = files.begin_write(&reference(), true).unwrap();
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
        let mut writer = files.begin_write(&reference(), true).unwrap();
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
        let mut reader = files.open_read(&reference()).unwrap();
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
        inner.files().create(&reference(), &bytes).unwrap();
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
        .open_read(&reference())
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
        let mut writer = files.begin_write(&reference(), true).unwrap();
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
    let mut reader: FileReader = files.open_read(&reference()).unwrap();
    assert_send_static(&reader);
    drop(files);
    drop(inner);
    let mut data = Vec::new();
    reader.read_to_end(&mut data).unwrap();
    assert_eq!(data, b"retained");
}
