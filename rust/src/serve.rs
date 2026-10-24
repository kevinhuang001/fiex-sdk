//! The stdio request loop shared by [`serve`] and [`serve_with_host`].
use crate::host::{HostClient, HostServices};
use crate::types::{read_frame, write_frame, RpcError, PROTOCOL_VERSION};
use serde_json::{json, Value};
use std::io::{self, BufReader};

/// Same external process protocol for all implementations, including third parties.
/// Native plugin commands can call the host's namespaced storage and credential
/// services through the provided HostClient.
pub fn serve_with_host(
    plugin_id: &str,
    mut dispatch: impl FnMut(&str, Value, &mut dyn HostServices) -> Result<Value, RpcError>,
) -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut initialized = false;
    while let Some(request) = read_frame(&mut input)? {
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let response = match method {
            "plugin.initialize" => {
                let version = params.get("protocolVersion").and_then(Value::as_str);
                if version != Some(PROTOCOL_VERSION) {
                    Err(RpcError::invalid_params("unsupported protocol version"))
                } else {
                    initialized = true;
                    Ok(
                        json!({"pluginId":plugin_id,"protocolVersion":PROTOCOL_VERSION,"ready":true}),
                    )
                }
            }
            "plugin.ping" if initialized => Ok(json!({"alive":true})),
            "plugin.shutdown" if initialized => {
                write_frame(&mut output, &json!({"jsonrpc":"2.0","id":id,"result":null}))?;
                break;
            }
            _ if !initialized => Err(RpcError::invalid_params("initialize required")),
            other => {
                let mut host = HostClient {
                    reader: &mut input,
                    writer: &mut output,
                    sequence: 0,
                };
                dispatch(other, params, &mut host)
            }
        };
        let message = match response {
            Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
            Err(error) => json!({"jsonrpc":"2.0","id":id,"error":error}),
        };
        write_frame(&mut output, &message)?;
    }
    Ok(())
}

/// Convenience wrapper for plugins without host-side service calls.
pub fn serve(
    plugin_id: &str,
    mut dispatch: impl FnMut(&str, Value) -> Result<Value, RpcError>,
) -> io::Result<()> {
    serve_with_host(plugin_id, |method, params, _host| dispatch(method, params))
}
