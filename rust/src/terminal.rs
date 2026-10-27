//! Typed wrapper around the host-owned terminal bridge.
//!
//! A terminal session is created by `terminal.open`; the plugin backend then
//! exchanges bytes and dimensions without knowing the JSON-RPC field names.
use crate::{context::ContextInner, RpcError};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;

#[derive(Clone)]
pub struct Terminal {
    context: ContextInner,
    session_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalInput {
    pub data: Vec<u8>,
    /// The host closed the terminal; no more output can be produced.
    pub eof: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

impl Terminal {
    pub(crate) fn new(context: ContextInner, session_id: impl Into<String>) -> Self {
        Self {
            context,
            session_id: session_id.into(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Tell the host the PTY is initialized and the terminal can be shown.
    pub fn ready(&self) -> Result<(), RpcError> {
        self.context
            .call("host.terminal.ready", json!({"sessionId":self.session_id}))?;
        Ok(())
    }

    /// Send raw PTY output to the host terminal.
    pub fn output(&self, data: &[u8]) -> Result<(), RpcError> {
        self.context.call(
            "host.terminal.output",
            json!({"sessionId":self.session_id,"dataBase64":STANDARD.encode(data)}),
        )?;
        Ok(())
    }

    /// Drain at most `max_bytes` of user input. `max_bytes` is clamped by Core.
    pub fn input(&self, max_bytes: usize) -> Result<TerminalInput, RpcError> {
        let value = self.context.call(
            "host.terminal.input",
            json!({"sessionId":self.session_id,"maxBytes":max_bytes}),
        )?;
        let data = value["dataBase64"]
            .as_str()
            .ok_or_else(|| RpcError::internal("invalid terminal input"))?;
        Ok(TerminalInput {
            data: STANDARD
                .decode(data)
                .map_err(|error| RpcError::internal(error.to_string()))?,
            eof: value["eof"].as_bool().unwrap_or(false),
        })
    }

    /// Current terminal size. The host clamps values to a usable range.
    pub fn size(&self) -> Result<TerminalSize, RpcError> {
        let value = self
            .context
            .call("host.terminal.size", json!({"sessionId":self.session_id}))?;
        let cols = value["cols"]
            .as_u64()
            .ok_or_else(|| RpcError::internal("invalid terminal columns"))?;
        let rows = value["rows"]
            .as_u64()
            .ok_or_else(|| RpcError::internal("invalid terminal rows"))?;
        Ok(TerminalSize {
            cols: cols.min(u16::MAX as u64) as u16,
            rows: rows.min(u16::MAX as u64) as u16,
        })
    }

    pub fn closed(&self) -> Result<bool, RpcError> {
        let value = self
            .context
            .call("host.terminal.closed", json!({"sessionId":self.session_id}))?;
        value["closed"]
            .as_bool()
            .ok_or_else(|| RpcError::internal("invalid terminal state"))
    }

    /// End the session. A message is surfaced as the terminal's closing error.
    pub fn end(&self, error: Option<&str>) -> Result<(), RpcError> {
        self.context.call(
            "host.terminal.end",
            json!({"sessionId":self.session_id,"error":error}),
        )?;
        Ok(())
    }
}
