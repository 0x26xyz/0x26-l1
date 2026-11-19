use crate::JsonRpcServerConfig;
use std::collections::BTreeMap;

pub(crate) fn operator_request_is_authorized(
    headers: &BTreeMap<String, String>,
    config: &JsonRpcServerConfig,
) -> bool {
    let Some(expected_token) = config.operator_bearer_token.as_deref() else {
        return true;
    };
    request_has_bearer_token(headers, expected_token)
}

pub(crate) fn peer_relay_request_is_authorized(
    headers: &BTreeMap<String, String>,
    config: &JsonRpcServerConfig,
) -> bool {
    if let Some(expected_token) = config.peer_relay_bearer_token.as_deref() {
        return request_has_bearer_token(headers, expected_token);
    }
    operator_request_is_authorized(headers, config)
}

fn request_has_bearer_token(headers: &BTreeMap<String, String>, expected_token: &str) -> bool {
    headers
        .get("authorization")
        .is_some_and(|authorization| authorization == &format!("Bearer {expected_token}"))
}

pub(crate) fn faucet_client_id(headers: &BTreeMap<String, String>) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .map(String::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("unknown")
        .to_ascii_lowercase()
}
