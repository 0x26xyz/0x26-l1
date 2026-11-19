use std::collections::BTreeMap;
use std::io::Read;
use std::net::TcpStream;

pub(crate) const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
pub(crate) const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum TransportError {
    Bind(std::io::Error),
    Accept(std::io::Error),
    Read(std::io::Error),
    Write(std::io::Error),
    RequestTooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HttpRequest {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: Option<String>,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: String,
}

pub(crate) fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, TransportError> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let read = stream.read(&mut chunk).map_err(TransportError::Read)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);

        if header_end.is_none() && buffer.len() > MAX_HTTP_HEADER_BYTES {
            return Err(TransportError::RequestTooLarge);
        }

        if header_end.is_none() {
            header_end = find_header_end(&buffer);
            if let Some(end) = header_end {
                if end > MAX_HTTP_HEADER_BYTES {
                    return Err(TransportError::RequestTooLarge);
                }
                content_length = parse_content_length(&buffer[..end]).unwrap_or(0);
                if content_length > MAX_HTTP_BODY_BYTES {
                    return Err(TransportError::RequestTooLarge);
                }
            }
        }

        if let Some(end) = header_end {
            let body_len = buffer.len().saturating_sub(end);
            if body_len > MAX_HTTP_BODY_BYTES {
                return Err(TransportError::RequestTooLarge);
            }
            if body_len >= content_length {
                let header_text = String::from_utf8_lossy(&buffer[..end]).to_string();
                let body = &buffer[end..end + content_length];
                return parse_http_request(&header_text, String::from_utf8_lossy(body).to_string())
                    .map_err(|message| {
                        TransportError::Read(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            message,
                        ))
                    });
            }
        }
    }

    Err(TransportError::Read(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "incomplete http request",
    )))
}

fn parse_http_request(headers: &str, body: String) -> Result<HttpRequest, &'static str> {
    let mut lines = headers.lines();
    let request_line = lines.next().ok_or("missing request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("missing http method")?;
    let target = parts.next().ok_or("missing request target")?;
    let _version = parts.next().ok_or("missing http version")?;
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (target.to_string(), None),
    };
    let mut header_map = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            header_map.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    Ok(HttpRequest {
        method: method.to_string(),
        path,
        query,
        headers: header_map,
        body,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn parse_content_length(headers: &[u8]) -> Option<usize> {
    let text = String::from_utf8_lossy(headers);
    text.lines().find_map(|line| {
        let mut parts = line.splitn(2, ':');
        let name = parts.next()?.trim();
        let value = parts.next()?.trim();
        if name.eq_ignore_ascii_case("content-length") {
            value.parse().ok()
        } else {
            None
        }
    })
}
