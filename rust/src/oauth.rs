//! Browser authorization-code OAuth. Core owns the browser, loopback listener,
//! CSRF state and PKCE; the plugin owns provider-specific token exchange.
use crate::{context::ContextInner, RpcError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthRequest {
    pub authorization_endpoint: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub parameters: BTreeMap<String, String>,
    /// Interactive browser budget; Core caps this at ten minutes.
    pub timeout_ms: u64,
}
impl OAuthRequest {
    pub fn new(
        endpoint: impl Into<String>,
        client_id: impl Into<String>,
        scopes: Vec<String>,
    ) -> Self {
        Self {
            authorization_endpoint: endpoint.into(),
            client_id: client_id.into(),
            scopes,
            parameters: BTreeMap::new(),
            timeout_ms: 300_000,
        }
    }
}
/// Sensitive, single-use exchange material. Deliberately does not implement Debug.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationCode {
    pub code: String,
    pub redirect_uri: String,
    pub code_verifier: String,
}
#[derive(Clone)]
pub struct OAuth {
    context: ContextInner,
}
impl OAuth {
    pub(crate) fn new(context: ContextInner) -> Self {
        Self { context }
    }
    pub fn authorize(&self, request: &OAuthRequest) -> Result<AuthorizationCode, RpcError> {
        let params =
            serde_json::to_value(request).map_err(|e| RpcError::invalid_params(e.to_string()))?;
        serde_json::from_value(self.context.call("host.oauth.authorize", params)?)
            .map_err(|_| RpcError::internal("invalid OAuth authorization response"))
    }
}
