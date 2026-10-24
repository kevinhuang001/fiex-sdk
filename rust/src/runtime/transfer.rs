//! Bounded automatic transfer policy for file and stream data.
//!
//! Small operations use the framed control channel. Larger reads map one
//! reusable shared window per open handle; larger writes map one reusable
//! staging window per open writer. Allocation failures fall back to bounded
//! IPC without changing the public reader/writer types.
use crate::{context::ContextInner, ipc, RpcError};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;
use std::io;

pub(crate) const CHUNK: usize = 256 * 1024;
pub(crate) const SHARED_WINDOW: usize = 4 * 1024 * 1024;

fn io_error(error: RpcError) -> io::Error {
    crate::context::rpc_to_io(error)
}

#[derive(Default)]
pub(crate) struct ReadCache {
    start: u64,
    bytes: Vec<u8>,
    misses: usize,
    shared: Option<(ipc::SharedBuffer, ipc::BufferDescriptor)>,
}

impl ReadCache {
    pub(crate) fn release(&mut self, context: &ContextInner) {
        if let Some((_, descriptor)) = self.shared.take() {
            if !context.closed() {
                let _ = context.call("host.buffer.release", json!({"id": descriptor.id}));
            }
        }
    }

    pub(crate) fn read(
        &mut self,
        context: &ContextInner,
        handle: &str,
        offset: &mut u64,
        output: &mut [u8],
    ) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let cached = offset
            .checked_sub(self.start)
            .and_then(|value| usize::try_from(value).ok());
        if cached.is_none_or(|index| index >= self.bytes.len()) {
            if (output.len() >= CHUNK || self.misses >= 2) && self.shared.is_none() {
                if let Ok(descriptor) = context
                    .call(
                        "host.buffer.create",
                        json!({"size": SHARED_WINDOW, "writable": false}),
                    )
                    .and_then(|value| {
                        serde_json::from_value::<ipc::BufferDescriptor>(value)
                            .map_err(|error| RpcError::internal(error.to_string()))
                    })
                {
                    match ipc::SharedBuffer::open(&descriptor) {
                        Ok(mapped) => self.shared = Some((mapped, descriptor)),
                        Err(_) => {
                            let _ =
                                context.call("host.buffer.release", json!({"id": descriptor.id}));
                        }
                    }
                }
            }
            let mut requested = if self.shared.is_some() {
                SHARED_WINDOW
            } else {
                CHUNK
            };
            let response = if let Some((_, descriptor)) = &self.shared {
                match context.call(
                    "host.fs.readOpen",
                    json!({"handle": handle, "offset": *offset, "length": requested, "buffer": descriptor.id}),
                ) {
                    Ok(response) => response,
                    Err(_) => {
                        // The window itself failed: fall back to IPC for this read.
                        self.release(context);
                        requested = CHUNK;
                        context
                            .call(
                                "host.fs.readOpen",
                                json!({"handle": handle, "offset": *offset, "length": requested}),
                            )
                            .map_err(io_error)?
                    }
                }
            } else {
                context
                    .call(
                        "host.fs.readOpen",
                        json!({"handle": handle, "offset": *offset, "length": requested}),
                    )
                    .map_err(io_error)?
            };
            let size = response["size"]
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| io::Error::other("invalid file chunk size"))?;
            if size > requested {
                return Err(io::Error::other("invalid file chunk size"));
            }
            let bytes = if let Some((mapped, _)) = &self.shared {
                mapped
                    .as_slice()
                    .get(..size)
                    .ok_or_else(|| io::Error::other("shared file window is too small"))?
                    .to_vec()
            } else {
                let encoded = response["base64"]
                    .as_str()
                    .ok_or_else(|| io::Error::other("missing file chunk"))?;
                let bytes = STANDARD.decode(encoded).map_err(io::Error::other)?;
                if bytes.len() != size {
                    return Err(io::Error::other("invalid file chunk size"));
                }
                bytes
            };
            self.start = *offset;
            self.bytes = bytes;
            self.misses = self.misses.saturating_add(1);
        }
        let start = (*offset - self.start) as usize;
        let size = output.len().min(self.bytes.len() - start);
        output[..size].copy_from_slice(&self.bytes[start..start + size]);
        *offset = offset
            .checked_add(size as u64)
            .ok_or_else(|| io::Error::other("file offset overflow"))?;
        Ok(size)
    }
}

pub(crate) fn flush_pending(
    context: &ContextInner,
    handle: &str,
    pending: &mut Vec<u8>,
    failed: &mut bool,
    shared: &mut Option<(ipc::SharedBuffer, ipc::BufferDescriptor)>,
) -> io::Result<()> {
    if *failed {
        return Err(io::Error::other("transaction failed"));
    }
    if pending.is_empty() {
        return Ok(());
    }
    if pending.len() >= 64 * 1024 && shared.is_none() {
        if let Ok(descriptor) = context
            .call(
                "host.buffer.create",
                json!({"size": CHUNK, "writable": true}),
            )
            .and_then(|value| {
                serde_json::from_value::<ipc::BufferDescriptor>(value)
                    .map_err(|error| RpcError::internal(error.to_string()))
            })
        {
            match ipc::SharedBuffer::open(&descriptor) {
                Ok(mapped) => *shared = Some((mapped, descriptor)),
                Err(_) => {
                    let _ = context.call("host.buffer.release", json!({"id": descriptor.id}));
                }
            }
        }
    }
    let result = if let Some((mapped, descriptor)) = shared.as_mut() {
        match mapped.as_mut_slice() {
            Ok(bytes) if bytes.len() >= pending.len() => {
                bytes[..pending.len()].copy_from_slice(pending);
                context.call(
                    "host.fs.writeBuffer",
                    json!({"handle": handle, "buffer": descriptor.id, "size": pending.len()}),
                )
            }
            _ => {
                release_write_buffer(context, shared);
                context.call(
                    "host.fs.writeChunk",
                    json!({"handle": handle, "base64": STANDARD.encode(&*pending)}),
                )
            }
        }
    } else {
        context.call(
            "host.fs.writeChunk",
            json!({"handle": handle, "base64": STANDARD.encode(&*pending)}),
        )
    };
    if let Err(error) = result {
        *failed = true;
        return Err(io_error(error));
    }
    pending.clear();
    Ok(())
}

pub(crate) fn release_write_buffer(
    context: &ContextInner,
    shared: &mut Option<(ipc::SharedBuffer, ipc::BufferDescriptor)>,
) {
    if let Some((_, descriptor)) = shared.take() {
        if !context.closed() {
            let _ = context.call("host.buffer.release", json!({"id": descriptor.id}));
        }
    }
}
