use super::*;

pub(super) fn wire_op(method: &str) -> Option<FsOperation> {
    FsOperation::ALL
        .into_iter()
        .find(|op| op.methods().contains(&method))
}

/// Invoke a provider method that opens a stream under a session that outlives
/// this request, so the later `vfs.readStream`/`vfs.writeStream`/`vfs.commit`
/// calls keep the host handles the provider opened (HTTP bodies, sockets, file
/// handles). Falls back to the request session when Core refuses a task.
pub(super) fn invoke_stream(
    provider: &dyn VfsProvider,
    ctx: &RequestContext,
    op: FsOperation,
    label: &str,
) -> Result<(FsProduct, Option<StreamSession>), RpcError> {
    require(provider, ctx, op)?;
    let session = StreamSession::open(ctx.inner(), label);
    let derived = session
        .as_ref()
        .map(|stream| ctx.derive(stream.context().clone()));
    let request = FsRequest::new(derived.as_ref().unwrap_or(ctx), op);
    let product = invoke(provider, &request)?;
    Ok((product, session))
}

pub(super) fn require(
    provider: &dyn VfsProvider,
    ctx: &RequestContext,
    op: FsOperation,
) -> Result<Vec<FsOperation>, RpcError> {
    let available = discover(provider, ctx)?;
    if available.contains(&op) {
        Ok(available)
    } else {
        let mut error = unsupported(op);
        error.data = Some(json!({"operation": op.as_str()}));
        Err(error)
    }
}

pub(super) fn narrow_entry(available: &[FsOperation], entry: &mut FsEntry) {
    let allowed = available
        .iter()
        .copied()
        .filter(|op| {
            entry
                .allowed_ops
                .as_ref()
                .is_none_or(|ops| ops.contains(op))
        })
        .collect();
    entry.allowed_ops = Some(allowed);
}

pub(super) fn param_str<'a>(ctx: &'a RequestContext, key: &str) -> Result<&'a str, RpcError> {
    ctx.params()
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::invalid_params(format!("missing {key}")))
}

pub(super) fn decode_chunk(ctx: &RequestContext) -> Result<Vec<u8>, RpcError> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let encoded = param_str(ctx, "base64")?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| RpcError::invalid_params("invalid base64 chunk"))?;
    if bytes.len() > MAX_CHUNK {
        return Err(RpcError::invalid_params("chunk exceeds limit"));
    }
    Ok(bytes)
}

pub(super) fn skip_to(
    reader: &mut Box<dyn Read + Send>,
    cursor: &mut u64,
    offset: u64,
) -> Result<(), RpcError> {
    let mut skip = offset - *cursor;
    let mut scratch = [0u8; 8192];
    while skip > 0 {
        let want = skip.min(scratch.len() as u64) as usize;
        let read = reader
            .read(&mut scratch[..want])
            .map_err(|error| RpcError::internal(error.to_string()))?;
        if read == 0 {
            return Err(RpcError::internal("stream ended before requested offset"));
        }
        skip -= read as u64;
        *cursor += read as u64;
    }
    Ok(())
}

pub(super) fn read_chunk(
    reader: &mut Box<dyn Read + Send>,
    max_bytes: usize,
) -> Result<(Vec<u8>, bool), RpcError> {
    let mut bytes = vec![0u8; max_bytes];
    let mut read = 0;
    while read < max_bytes {
        match reader.read(&mut bytes[read..]) {
            Ok(0) => break,
            Ok(count) => read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(RpcError::internal(error.to_string())),
        }
    }
    bytes.truncate(read);
    // A short read means the provider's stream ended; Core can stop seeking
    // forward as soon as it sees eof.
    Ok((bytes, read < max_bytes))
}
