use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;

use crate::{parse_env_bool_with_default, parse_env_u64_with_default};

pub(crate) const X402_VERSION: u8 = 2;
pub(crate) const X402_PAYMENT_REQUIRED_HEADER: &str = "PAYMENT-REQUIRED";
pub(crate) const X402_PAYMENT_RESPONSE_HEADER: &str = "PAYMENT-RESPONSE";
pub(crate) const X402_PAYMENT_SIGNATURE_HEADER_LOWER: &str = "payment-signature";
const X402_LEGACY_PAYMENT_HEADER_LOWER: &str = "x-payment";
const X402_ENABLED_ENV: &str = "KOMBAT_X402_ENABLED";
const X402_RESOURCE_URL_ENV: &str = "KOMBAT_X402_RESOURCE_URL";
const X402_DESCRIPTION_ENV: &str = "KOMBAT_X402_DESCRIPTION";
const X402_MIME_TYPE_ENV: &str = "KOMBAT_X402_MIME_TYPE";
const X402_SCHEME_ENV: &str = "KOMBAT_X402_SCHEME";
const X402_NETWORK_ENV: &str = "KOMBAT_X402_NETWORK";
const X402_AMOUNT_ENV: &str = "KOMBAT_X402_AMOUNT";
const X402_ASSET_ENV: &str = "KOMBAT_X402_ASSET";
const X402_PAY_TO_ENV: &str = "KOMBAT_X402_PAY_TO";
const X402_MAX_TIMEOUT_SECONDS_ENV: &str = "KOMBAT_X402_MAX_TIMEOUT_SECONDS";
const X402_EXTRA_JSON_ENV: &str = "KOMBAT_X402_EXTRA_JSON";
const X402_FACILITATOR_URL_ENV: &str = "KOMBAT_X402_FACILITATOR_URL";
const X402_METHODS_ENV: &str = "KOMBAT_X402_METHODS";
const X402_ALLOW_UNVERIFIED_ENV: &str = "KOMBAT_X402_ALLOW_UNVERIFIED";
const X402_SETTLE_ENV: &str = "KOMBAT_X402_SETTLE";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402ResourceInfo {
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(rename = "mimeType", default, skip_serializing_if = "String::is_empty")]
    pub mime_type: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct X402PaymentRequirement {
    pub scheme: String,
    pub network: String,
    pub asset: String,
    pub amount: String,
    #[serde(rename = "payTo")]
    pub pay_to: String,
    #[serde(rename = "maxTimeoutSeconds")]
    pub max_timeout_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct X402PaymentRequired {
    #[serde(rename = "x402Version")]
    pub x402_version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub resource: X402ResourceInfo,
    pub accepts: Vec<X402PaymentRequirement>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct X402Config {
    pub resource: X402ResourceInfo,
    pub accepts: Vec<X402PaymentRequirement>,
    pub facilitator_url: Option<String>,
    pub protected_methods: BTreeSet<String>,
    pub allow_unverified: bool,
    pub settle: bool,
}

impl X402Config {
    pub(crate) fn payment_required(&self, error: Option<String>) -> X402PaymentRequired {
        X402PaymentRequired {
            x402_version: X402_VERSION,
            error,
            resource: self.resource.clone(),
            accepts: self.accepts.clone(),
        }
    }

    pub(crate) fn protects_method(&self, method: &str) -> bool {
        self.protected_methods.is_empty() || self.protected_methods.contains(method)
    }
}

pub(crate) fn parse_config_from_env() -> Result<Option<X402Config>, String> {
    if !parse_env_bool_with_default(X402_ENABLED_ENV, false) {
        return Ok(None);
    }

    let required_env = |name: &str| -> Result<String, String> {
        env::var(name)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{name} is required when {X402_ENABLED_ENV}=1"))
    };

    let resource = X402ResourceInfo {
        url: required_env(X402_RESOURCE_URL_ENV)?,
        description: env::var(X402_DESCRIPTION_ENV)
            .unwrap_or_else(|_| "Kombat Chain JSON-RPC".into())
            .trim()
            .to_owned(),
        mime_type: env::var(X402_MIME_TYPE_ENV)
            .unwrap_or_else(|_| "application/json".into())
            .trim()
            .to_owned(),
    };
    let extra = match env::var(X402_EXTRA_JSON_ENV) {
        Ok(value) if !value.trim().is_empty() => Some(
            serde_json::from_str(value.trim())
                .map_err(|error| format!("{X402_EXTRA_JSON_ENV} must be valid JSON: {error}"))?,
        ),
        _ => None,
    };
    let accepts = vec![X402PaymentRequirement {
        scheme: env::var(X402_SCHEME_ENV)
            .unwrap_or_else(|_| "exact".into())
            .trim()
            .to_owned(),
        network: required_env(X402_NETWORK_ENV)?,
        asset: required_env(X402_ASSET_ENV)?,
        amount: required_env(X402_AMOUNT_ENV)?,
        pay_to: required_env(X402_PAY_TO_ENV)?,
        max_timeout_seconds: parse_env_u64_with_default(X402_MAX_TIMEOUT_SECONDS_ENV, 60),
        extra,
    }];
    let protected_methods = env::var(X402_METHODS_ENV)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|method| !method.is_empty())
        .map(str::to_owned)
        .collect();
    let allow_unverified = parse_env_bool_with_default(X402_ALLOW_UNVERIFIED_ENV, false);
    let facilitator_url = env::var(X402_FACILITATOR_URL_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .filter(|value| !value.is_empty());
    if !allow_unverified && facilitator_url.is_none() {
        return Err(format!(
            "{X402_FACILITATOR_URL_ENV} is required unless {X402_ALLOW_UNVERIFIED_ENV}=1"
        ));
    }

    Ok(Some(X402Config {
        resource,
        accepts,
        facilitator_url,
        protected_methods,
        allow_unverified,
        settle: parse_env_bool_with_default(X402_SETTLE_ENV, true),
    }))
}

pub(crate) fn encode_header_value<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| format!("encode x402 json: {error}"))?;
    Ok(BASE64_STANDARD.encode(bytes))
}

pub(crate) fn decode_header_value(value: &str) -> Result<Value, String> {
    let bytes = BASE64_STANDARD
        .decode(value.trim())
        .map_err(|error| format!("decode x402 header: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("decode x402 json: {error}"))
}

pub(crate) fn payment_header(headers: &BTreeMap<String, String>) -> Option<&str> {
    headers
        .get(X402_PAYMENT_SIGNATURE_HEADER_LOWER)
        .or_else(|| headers.get(X402_LEGACY_PAYMENT_HEADER_LOWER))
        .map(String::as_str)
}

pub(crate) fn response_success(value: &Value) -> bool {
    ["success", "isValid", "valid"]
        .iter()
        .any(|key| value.get(*key).and_then(Value::as_bool) == Some(true))
}

pub(crate) fn response_error(value: &Value, fallback: &str) -> String {
    [
        "errorMessage",
        "invalidMessage",
        "errorReason",
        "invalidReason",
        "error",
    ]
    .iter()
    .find_map(|key| value.get(*key).and_then(Value::as_str))
    .filter(|message| !message.is_empty())
    .unwrap_or(fallback)
    .to_owned()
}

fn requirement_matches(left: &X402PaymentRequirement, right: &X402PaymentRequirement) -> bool {
    left.scheme == right.scheme
        && left.network == right.network
        && left.asset == right.asset
        && left.amount == right.amount
        && left.pay_to == right.pay_to
        && left.max_timeout_seconds == right.max_timeout_seconds
        && left.extra == right.extra
}

pub(crate) fn select_requirement<'a>(
    config: &'a X402Config,
    payment_payload: &Value,
) -> Result<&'a X402PaymentRequirement, String> {
    let Some(accepted) = payment_payload.get("accepted") else {
        return config
            .accepts
            .first()
            .ok_or_else(|| "x402 config has no payment requirements".into());
    };
    let accepted: X402PaymentRequirement = serde_json::from_value(accepted.clone())
        .map_err(|error| format!("decode x402 accepted requirement: {error}"))?;
    config
        .accepts
        .iter()
        .find(|requirement| requirement_matches(requirement, &accepted))
        .ok_or_else(|| "x402 payment does not match any accepted requirement".into())
}
