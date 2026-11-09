use super::mock::{context, Mock};
use crate::tcp::TcpStream;
use std::{
    io::{Read, Write},
    sync::Arc,
};

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
