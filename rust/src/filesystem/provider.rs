//! SDK stream dispatch helpers around the canonical shared VFS trait.
use super::*;
use crate::RpcError;
use fiex_provider_contracts::ProviderInvocation;
pub use fiex_provider_contracts::VfsProvider;

pub(super) fn unsupported(op: FsOperation) -> RpcError {
    let mut error = RpcError::method_not_found(op.methods()[0]);
    error.message = format!("Unsupported fs operation: {}", op.as_str());
    error
}

pub(super) fn invoke(
    provider: &dyn VfsProvider,
    request: &FsRequest<'_>,
) -> Result<FsProduct, RpcError> {
    Ok(match request.op() {
        FsOperation::List => FsProduct::Listing(provider.list(request)?),
        FsOperation::Stat => FsProduct::Entry(provider.stat(request)?),
        FsOperation::Read => FsProduct::Read(provider.read(request)?),
        FsOperation::Write => FsProduct::Write(provider.write(request)?),
        FsOperation::Mkdir => FsProduct::Entry(provider.mkdir(request)?),
        FsOperation::Rename => FsProduct::Entry(provider.rename(request)?),
        FsOperation::Remove => {
            provider.remove(request)?;
            FsProduct::Done
        }
        FsOperation::Search => FsProduct::Listing(provider.search(request)?),
    })
}

pub(super) fn discover(
    provider: &dyn VfsProvider,
    ctx: &dyn ProviderInvocation,
) -> Result<Vec<FsOperation>, RpcError> {
    let reported = provider.discover(ctx)?;
    Ok(provider
        .operations()
        .iter()
        .copied()
        .filter(|op| reported.contains(op))
        .collect())
}
