use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use url::Url;

use super::base::{FetchOptions, ProtocolVersion, Transport};
use super::transport;

pub const DEFAULT_UTNP_PORT: u16 = 34;

#[derive(Debug, Clone)]
pub struct Target {
    pub protocol: &'static str,
    pub host: String,
    pub path_query: String,
    pub direct_port: Option<u16>,
    pub utnp_port: Option<u16>,
    pub transport: Transport,
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub version: ProtocolVersion,
    pub transport: Transport,
}

impl Target {
    pub fn parse(input: &str) -> Result<Self> {
        let (normalized, utnp_port) = normalize_u_port(input)?;

        let url = Url::parse(&normalized)
            .with_context(|| format!("Invalid TextNet URI: {input}"))?;

        let (protocol, transport) = match url.scheme().to_ascii_lowercase().as_str() {
            "tmutp" => ("tmutp", Transport::Tls),
            "tmutps" => ("tmutp", Transport::Ssh),
            "tbip" => ("tbip", Transport::Tls),
            "tbips" => ("tbip", Transport::Ssh),
            other => bail!("Unsupported URI scheme: {other}"),
        };

        let host = url
            .host_str()
            .ok_or_else(|| anyhow!("URI does not contain a host"))?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();

        let path = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };

        let mut path_query = path.to_string();

        if let Some(query) = url.query() {
            path_query.push('?');
            path_query.push_str(query);
        }

        let direct_port = if utnp_port.is_none() {
            url.port()
        } else {
            None
        };

        Ok(Self {
            protocol,
            host,
            path_query,
            direct_port,
            utnp_port,
            transport,
        })
    }
}

fn normalize_u_port(input: &str) -> Result<(String, Option<u16>)> {
    let (scheme, rest) = input
        .split_once("://")
        .ok_or_else(|| anyhow!("URI must contain ://"))?;

    let authority_end = rest
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());

    let authority = &rest[..authority_end];
    let suffix = &rest[authority_end..];

    let Some((host_part, port_part)) = authority.rsplit_once(':') else {
        return Ok((input.to_string(), None));
    };

    if !port_part.starts_with('u') {
        return Ok((input.to_string(), None));
    }

    let digits = &port_part[1..];

    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        bail!("Invalid UTNP port: {port_part}");
    }

    let port: u16 = digits
        .parse()
        .context("UTNP port is outside the valid range")?;

    if port == 0 {
        bail!("UTNP port cannot be zero");
    }

    let normalized = format!("{scheme}://{host_part}:{port}{suffix}");

    Ok((normalized, Some(port)))
}

pub async fn negotiate(
    target: &Target,
    options: &FetchOptions,
    ssh_key_store: Option<PathBuf>,
) -> Result<Endpoint> {
    let utnp_port = target.utnp_port.unwrap_or(DEFAULT_UTNP_PORT);

    let supported = if options.allow_v1 {
        "2.0,1.0"
    } else {
        "2.0"
    };

    let request = format!(
        "UTNP/2.0 NEGOTIATE {}\nSupported-Versions: {}\n\n",
        target.protocol, supported
    );

    let response = transport::exchange_negotiation(
        &target.host,
        utnp_port,
        request.as_bytes(),
        options.timeout_secs,
        ssh_key_store.clone(),
        target.transport,
    )
    .await
    .context("UTNP negotiation failed")?;

    let text = String::from_utf8_lossy(&response).into_owned();

    if text.starts_with("UTNP ERROR - UNSUPPORTED UTNP VERSION") {
        if !options.allow_v1 {
            bail!(
                "The server does not support TNet 2.0 and TNet 1.0 support is disabled"
            );
        }

        return negotiate_legacy(target, options, utnp_port, ssh_key_store).await;
    }

    finish_negotiation(target, options, &text)
}

async fn negotiate_legacy(
    target: &Target,
    options: &FetchOptions,
    utnp_port: u16,
    ssh_key_store: Option<PathBuf>,
) -> Result<Endpoint> {
    let request = format!("UTNP/1.0 NEGOTIATE {}\n", target.protocol);

    let response = transport::exchange_negotiation(
        &target.host,
        utnp_port,
        request.as_bytes(),
        options.timeout_secs,
        ssh_key_store,
        Transport::Tcp,
    )
    .await
    .context("UTNP 1.0 negotiation failed")?;

    let text = String::from_utf8_lossy(&response).into_owned();

    finish_negotiation(target, options, &text)
}

fn finish_negotiation(
    target: &Target,
    options: &FetchOptions,
    text: &str,
) -> Result<Endpoint> {
    let first_line = text
        .lines()
        .next()
        .ok_or_else(|| anyhow!("UTNP returned an empty response"))?
        .trim();

    let parts: Vec<&str> = first_line.split_whitespace().collect();

    if parts.len() != 7
        || parts[1] != "START"
        || parts[2] != "PROTOCOL"
    {
        bail!("UTNP negotiation was rejected: {first_line}");
    }

    let utnp_version = match parts[0] {
        "UTNP/2.0" => ProtocolVersion::V2,
        "UTNP/1.0" => ProtocolVersion::V1,
        other => bail!("Unsupported UTNP version in response: {other}"),
    };

    if utnp_version == ProtocolVersion::V1 && !options.allow_v1 {
        bail!("The server selected UTNP/1.0 but TNet 1.0 support is disabled");
    }

    if !parts[3].eq_ignore_ascii_case(target.protocol) {
        bail!("UTNP returned an unexpected protocol: {}", parts[3]);
    }

    let protocol_version = match parts[4] {
        "2.0" => ProtocolVersion::V2,
        "1.0" => ProtocolVersion::V1,
        other => bail!("Unsupported protocol version in response: {other}"),
    };

    if protocol_version == ProtocolVersion::V1 && !options.allow_v1 {
        bail!("The server selected version 1.0 but TNet 1.0 support is disabled");
    }

    let host = parts[5].to_string();

    if host.is_empty() {
        bail!("UTNP returned an empty protocol host");
    }

    let port: u16 = parts[6]
        .parse()
        .context("UTNP returned an invalid protocol port")?;

    if port == 0 {
        bail!("UTNP returned port 0");
    }

    let transport = if protocol_version == ProtocolVersion::V1 {
        Transport::Tcp
    } else {
        check_security(target, options, text)?;
        target.transport
    };

    Ok(Endpoint {
        host,
        port,
        version: protocol_version,
        transport,
    })
}

fn check_security(
    target: &Target,
    options: &FetchOptions,
    text: &str,
) -> Result<()> {
    let advertised = parse_security(text);

    if advertised.is_empty() {
        return Ok(());
    }

    let required = match target.transport {
        Transport::Tls => "tls",
        Transport::Ssh => "ssh",
        Transport::Tcp => return Ok(()),
    };

    let supported = advertised
        .iter()
        .any(|mechanism| mechanism.eq_ignore_ascii_case(required));

    if supported {
        return Ok(());
    }

    if options.try_encryption_against_utnp {
        return Ok(());
    }

    let scheme_hint = match (required, target.protocol) {
        ("tls", "tmutp") => " (use a tmutps:// URL)",
        ("tls", "tbip") => " (use a tbips:// URL)",
        _ => "",
    };

    bail!(
        "UTNP security mismatch: {} is required but the server supports: {}{scheme_hint}",
        required,
        advertised.join(", ")
    )
}

fn parse_security(response: &str) -> Vec<String> {
    response
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;

            if !name.trim().eq_ignore_ascii_case("Security") {
                return None;
            }

            Some(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_ascii_lowercase())
                    .collect(),
            )
        })
        .unwrap_or_default()
}
