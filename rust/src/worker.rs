//! Run a synchronous protocol library on a worker while the serving thread
//! brokers its host calls. The worker has no socket or file privileges.
use crate::{HostServices, RpcError};
use serde_json::Value;
use std::sync::mpsc;
type Call = (String, Value, mpsc::Sender<Result<Value, RpcError>>);
#[derive(Clone)]
pub struct HostProxy {
    sender: mpsc::Sender<Call>,
}
impl HostServices for HostProxy {
    fn worker_proxy(&self) -> Option<HostProxy> {
        Some(self.clone())
    }
    fn invoke(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        let (sender, receiver) = mpsc::channel();
        self.sender
            .send((method.into(), params, sender))
            .map_err(|_| RpcError::internal("host worker disconnected"))?;
        receiver
            .recv()
            .map_err(|_| RpcError::internal("host worker disconnected"))?
    }
}
pub fn with_host_worker<T: Send>(
    host: &mut dyn HostServices,
    action: impl FnOnce(HostProxy) -> Result<T, RpcError> + Send,
) -> Result<T, RpcError> {
    std::thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel::<Call>();
        let worker = scope.spawn(move || action(HostProxy { sender }));
        for (method, params, reply) in receiver {
            let _ = reply.send(host.invoke(&method, params));
        }
        worker
            .join()
            .map_err(|_| RpcError::internal("protocol worker panicked"))?
    })
}
