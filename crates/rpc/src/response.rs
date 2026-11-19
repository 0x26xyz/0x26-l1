use crate::http::TransportError;
use crate::jsonrpc::JsonRpcErrorObject;
use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Response, StatusCode};
use std::collections::BTreeMap;
use std::io::Write;
use std::net::TcpStream;

const DEFAULT_BROWSER_CORS_ORIGIN: &str = "https://0x26.xyz";
const BROWSER_CORS_ALLOWED_ORIGINS: &[&str] = &[
    DEFAULT_BROWSER_CORS_ORIGIN,
    "https://testnet.0x26.xyz",
    "http://localhost:3000",
    "http://127.0.0.1:3000",
    "http://localhost:5173",
    "http://127.0.0.1:5173",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JsonHttpResponse {
    pub(crate) status: StatusCode,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) content_type: &'static str,
    pub(crate) body: String,
}

pub(crate) fn header_map_from_axum(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_ascii_lowercase(), value.to_string()))
        })
        .collect()
}

pub(crate) fn browser_cors_origin(headers: &BTreeMap<String, String>) -> String {
    headers
        .get("origin")
        .filter(|origin| {
            BROWSER_CORS_ALLOWED_ORIGINS
                .iter()
                .any(|allowed| origin.as_str() == *allowed)
        })
        .cloned()
        .unwrap_or_else(|| DEFAULT_BROWSER_CORS_ORIGIN.to_string())
}

pub(crate) fn json_response(
    status: StatusCode,
    content_type: &'static str,
    body: String,
) -> Response<Body> {
    json_response_with_headers(status, content_type, &BTreeMap::new(), body)
}

fn json_response_with_headers(
    status: StatusCode,
    content_type: &'static str,
    headers: &BTreeMap<String, String>,
    body: String,
) -> Response<Body> {
    json_response_with_browser_headers(status, content_type, headers, &BTreeMap::new(), body)
}

pub(crate) fn json_response_with_browser_headers(
    status: StatusCode,
    content_type: &'static str,
    headers: &BTreeMap<String, String>,
    request_headers: &BTreeMap<String, String>,
    body: String,
) -> Response<Body> {
    let cors_origin = browser_cors_origin(request_headers);
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, cors_origin)
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS")
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            "content-type, authorization, last-event-id",
        )
        .header(header::VARY, "Origin");
    for (name, value) in headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    builder
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::from("internal server error")))
}

pub(crate) fn insert_browser_sse_headers(
    headers: &mut HeaderMap,
    request_headers: &BTreeMap<String, String>,
) {
    let cors_origin = browser_cors_origin(request_headers);
    if let Ok(value) = HeaderValue::from_str(&cors_origin) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, authorization, last-event-id"),
    );
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
}

pub(crate) fn plain_response(status: StatusCode, body: &str) -> Response<Body> {
    json_response(status, "text/plain", body.to_string())
}

pub(crate) fn network_json_response(result: Result<String, JsonRpcErrorObject>) -> Response<Body> {
    match result {
        Ok(body) => json_response(StatusCode::OK, "application/json", body),
        Err(error) => {
            let status = if error.code == -32010 {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::BAD_REQUEST
            };
            plain_response(status, &error.message)
        }
    }
}

pub(crate) fn transport_error_message(error: &TransportError) -> String {
    format!("{error:?}")
}

pub(crate) fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> Result<(), TransportError> {
    write_http_response_with_headers(stream, status, content_type, &BTreeMap::new(), body)
}

pub(crate) fn write_http_response_for_request(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    request_headers: &BTreeMap<String, String>,
    body: &str,
) -> Result<(), TransportError> {
    write_http_response_with_headers_for_request(
        stream,
        status,
        content_type,
        &BTreeMap::new(),
        request_headers,
        body,
    )
}

fn write_http_response_with_headers(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    headers: &BTreeMap<String, String>,
    body: &str,
) -> Result<(), TransportError> {
    write_http_response_with_headers_for_request(
        stream,
        status,
        content_type,
        headers,
        &BTreeMap::new(),
        body,
    )
}

pub(crate) fn write_http_response_with_headers_for_request(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    headers: &BTreeMap<String, String>,
    request_headers: &BTreeMap<String, String>,
    body: &str,
) -> Result<(), TransportError> {
    let cors_origin = browser_cors_origin(request_headers);
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: {cors_origin}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: content-type, authorization, last-event-id\r\nVary: Origin\r\n",
        body.len(),
    );
    for (name, value) in headers {
        response.push_str(name);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    response.push_str(body);
    stream
        .write_all(response.as_bytes())
        .map_err(TransportError::Write)
}

pub(crate) fn http_status_line(status: StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("{} {reason}", status.as_u16()),
        None => status.as_u16().to_string(),
    }
}

pub(crate) fn write_sse_headers(
    stream: &mut TcpStream,
    request_headers: &BTreeMap<String, String>,
) -> Result<(), TransportError> {
    let cors_origin = browser_cors_origin(request_headers);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache, no-transform\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: {cors_origin}\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: content-type, authorization, last-event-id\r\nVary: Origin\r\nX-Accel-Buffering: no\r\n\r\n"
    );
    stream
        .write_all(response.as_bytes())
        .map_err(TransportError::Write)?;
    stream.flush().map_err(TransportError::Write)
}
