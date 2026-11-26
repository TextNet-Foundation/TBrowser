use std::collections::HashMap;

use anyhow::{bail, Context, Result};

use super::base::ProtocolVersion;
use super::transport::response_header_end;

pub const ACCEPT_COMPRESSION: &str = "zstd";

pub fn status_name(code: u16) -> &'static str {
    match code {
        20 => "OK",
        21 => "MOVED",
        22 => "OK EMPTY",
        23 => "MULTIPLE CHOICES",
        30 => "BAD REQUEST",
        31 => "NOT FOUND",
        32 => "FORBIDDEN",
        40 => "SERVER ERROR",
        41 => "NOT IMPLEMENTED",
        42 => "SERVICE UNAVAILABLE",
        50 => "METHOD NOT ALLOWED",
        51 => "VERSION NOT SUPPORTED",
        52 => "INVALID RANGE",
        53 => "RANGE NOT SATISFIABLE",
        54 => "UNSUPPORTED RANGE",
        55 => "REQUIRE SECURE",
        _ => "UNKNOWN",
    }
}

pub struct WireResponse {
    pub status_line: String,
    pub code: u16,
    pub name: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub fn build_request(
    protocol: &str,
    version: ProtocolVersion,
    method: &str,
    host: &str,
    path_query: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> Vec<u8> {
    let mut request = format!(
        "{protocol}/{} {method} {path_query}\n\
         Host: {host}\n\
         User-Agent: tbrowser\n\
         Accept-Compression: {ACCEPT_COMPRESSION}\n",
        version.number(),
    );

    if let Some(content_type) = content_type {
        request.push_str(&format!("Content-Type: {content_type}\n"));
    }

    request.push_str(&format!("Content-Length: {}\n\n", body.len()));

    let mut bytes = request.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

pub fn parse_response(raw: &[u8]) -> Result<WireResponse> {
    let header_end = response_header_end(raw)
        .ok_or_else(|| anyhow::anyhow!("Server returned an incomplete response"))?;

    let header_text = String::from_utf8_lossy(&raw[..header_end]).into_owned();
    let body = raw[header_end..].to_vec();

    let mut lines = header_text.lines();

    let status_line = lines
        .next()
        .map(|line| line.trim_end_matches('\r').to_string())
        .ok_or_else(|| anyhow::anyhow!("Server returned an empty status line"))?;

    let parts: Vec<&str> = status_line.split_whitespace().collect();

    if parts.len() < 2 {
        bail!("Malformed status line: {status_line}");
    }

    let code: u16 = parts[1].parse().context("Invalid status code")?;

    let name = if parts.len() > 2 {
        parts[2..].join(" ")
    } else {
        status_name(code).to_string()
    };

    let mut headers = HashMap::new();

    for line in lines {
        let line = line.trim_end_matches('\r');

        if line.is_empty() {
            continue;
        }

        if let Some((key, value)) = line.split_once(':') {
            headers.insert(
                key.trim().to_ascii_lowercase(),
                value.trim().to_string(),
            );
        }
    }

    Ok(WireResponse {
        status_line,
        code,
        name,
        headers,
        body,
    })
}

pub fn decode_body(headers: &HashMap<String, String>, body: &[u8]) -> Result<Vec<u8>> {
    let Some(encoding) = headers.get("content-encoding") else {
        return Ok(body.to_vec());
    };

    match encoding.trim().to_ascii_lowercase().as_str() {
        "" | "identity" => Ok(body.to_vec()),

        "zstd" => zstd::stream::decode_all(body)
            .context("Cannot decompress zstd response body"),

        other => bail!("Unsupported response encoding: {other}"),
    }
}

pub fn header_u64(headers: &HashMap<String, String>, key: &str) -> Option<u64> {
    headers.get(key)?.trim().parse().ok()
}
