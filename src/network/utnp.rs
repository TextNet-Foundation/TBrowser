use std::collections::HashMap;
use std::time::Duration;

use anyhow::{bail, Context};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use url::Url;

use crate::network::base::{connect_host, Response};

pub const DEFAULT_UTNP_PORT: u16 = 34;
pub const DEFAULT_TMUTP_PORT: u16 = 100;
pub const DEFAULT_TBIP_PORT: u16 = 81;

/// The wire version this client speaks. UTNP may report an endpoint
/// running a newer protocol version, which cannot be used here.
const SUPPORTED_PROTOCOL_VERSION: &str = "1.0";

const NEGOTIATION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct UtnpClient;

impl UtnpClient {
    pub async fn fetch(url: &str) -> anyhow::Result<Response> {
        let parsed = Url::parse(url).context("invalid URL")?;
        let host = connect_host(&parsed);
        let port = parsed.port().unwrap_or(DEFAULT_UTNP_PORT);
        let path = parsed.path().trim_start_matches('/');

        if path.is_empty() {
            bail!("Missing command in UTNP URL");
        }
        let (command, arg) = match path.split_once('/') {
            Some((c, a)) => (c.to_ascii_lowercase(), Some(a.to_string())),
            None => (path.to_ascii_lowercase(), None),
        };

        if command != "list" && command != "negotiate" {
            bail!("Unknown UTNP command: {}", command);
        }
        if command == "negotiate" && arg.is_none() {
            bail!("negotiate requires protocol list");
        }

        let mut req = format!("UTNP/1.0 {}", command.to_ascii_uppercase());
        if let Some(a) = &arg {
            req.push(' ');
            req.push_str(a);
        }
        req.push('\n');

        // Подключаемся
        let mut sock = TcpStream::connect((host.as_str(), port)).await
            .with_context(|| format!("connect {}:{}", host, port))?;
        sock.write_all(req.as_bytes()).await?;
        sock.flush().await?;

        // Half-close: сообщаем серверу, что запрос завершён.
        // Без этого сервер ждёт следующий запрос и не закрывает соединение.
        sock.shutdown().await?;

        let mut buf = Vec::new();
        sock.read_to_end(&mut buf).await?;

        let text = String::from_utf8_lossy(&buf).into_owned();
        let mut lines = text.lines();
        let status_line = lines
            .next()
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("Empty UTNP response"))?;
        let body = lines.collect::<Vec<_>>().join("\n");

        let code: u16 = if status_line.starts_with("UTNP/1.0 START PROTOCOL")
            || status_line.starts_with("UTNP/1.0 AVAILABLE")
        {
            20
        } else {
            30
        };
        let name = if code == 20 { "OK" } else { "ERROR" }.to_string();

        Ok(Response {
            status_line,
            status_code: code,
            status_name: name,
            headers: HashMap::new(),
            body: body.clone().into_bytes(),
            text: body,
        })
    }
}

/// Formats a bare host for use inside an URL authority.
/// IPv6 literals must be wrapped in brackets, otherwise `Url::parse` fails.
fn url_host(host: &str) -> String {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{}]", bare)
    } else {
        bare.to_string()
    }
}

/// Resolves a protocol endpoint through UTNP.
///
/// Falls back to `host:default_port` when UTNP is unreachable or selects a
/// protocol version this client cannot speak. Such a fallback keeps legacy
/// `protocol://host/` URLs working on servers without UTNP or with a newer
/// default protocol version.
pub async fn negotiate_or_default(
    host: &str,
    protocol: &str,
    utnp_port: u16,
    default_port: u16,
) -> (String, u16) {
    match timeout(
        NEGOTIATION_TIMEOUT,
        negotiate_protocol(host, protocol, utnp_port),
    )
    .await
    {
        Ok(Ok(endpoint)) => endpoint,
        Ok(Err(_)) | Err(_) => (host.to_string(), default_port),
    }
}

pub async fn negotiate_protocol(
    host: &str,
    protocol: &str,
    utnp_port: u16,
) -> anyhow::Result<(String, u16)> {
    let url = format!(
        "utnp://{}:{}/negotiate/{}",
        url_host(host),
        utnp_port,
        protocol
    );
    let resp = UtnpClient::fetch(&url).await?;

    if !resp.ok() {
        bail!("UTNP negotiation failed: {}", resp.status_line);
    }

    // START PROTOCOL в статусной строке
    if resp.status_line.starts_with("UTNP/1.0 START PROTOCOL") {
        let parts: Vec<&str> = resp.status_line.split_whitespace().collect();
        if parts.len() >= 7 && parts[3].eq_ignore_ascii_case(protocol) {
            if parts[4] != SUPPORTED_PROTOCOL_VERSION {
                bail!(
                    "UTNP selected {} {}, but this client supports only {}",
                    protocol,
                    parts[4],
                    SUPPORTED_PROTOCOL_VERSION
                );
            }
            let proto_host = parts[5].to_string();
            let proto_port: u16 = parts[6].parse().context("bad port in status line")?;
            return Ok((proto_host, proto_port));
        }
    }

    // Иначе ищем в теле (AVAILABLE)
    for line in resp.text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4
            && parts[0].eq_ignore_ascii_case(protocol)
            && parts[1] == SUPPORTED_PROTOCOL_VERSION
        {
            let proto_host = parts[2].to_string();
            let proto_port: u16 = parts[3].parse().context("bad port in body")?;
            return Ok((proto_host, proto_port));
        }
    }

    bail!("Protocol {} not found in UTNP response", protocol);
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn serve_once(response: &'static str) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 512];
            let _ = sock.read(&mut buf).await;
            sock.write_all(response.as_bytes()).await.unwrap();
            sock.shutdown().await.unwrap();
        });
        port
    }

    #[tokio::test]
    async fn negotiate_parses_start_protocol() {
        let port = serve_once("UTNP/1.0 START PROTOCOL tmutp 1.0 10.0.0.7 34001\n").await;
        let (host, proto_port) = negotiate_protocol("127.0.0.1", "tmutp", port).await.unwrap();
        assert_eq!(host, "10.0.0.7");
        assert_eq!(proto_port, 34001);
    }

    #[tokio::test]
    async fn negotiate_accepts_protocol_name_regardless_of_case() {
        let port = serve_once("UTNP/1.0 START PROTOCOL TMUTP 1.0 10.0.0.8 34002\n").await;
        let (host, proto_port) = negotiate_protocol("127.0.0.1", "tmutp", port).await.unwrap();
        assert_eq!(host, "10.0.0.8");
        assert_eq!(proto_port, 34002);
    }

    #[tokio::test]
    async fn negotiate_falls_back_to_available_body() {
        let port = serve_once(
            "UTNP/1.0 AVAILABLE\nTMUTP 1.0 10.0.0.9 34003\nTBIP 1.0 10.0.0.9 34004\n",
        )
        .await;
        let (host, proto_port) = negotiate_protocol("127.0.0.1", "tmutp", port).await.unwrap();
        assert_eq!(host, "10.0.0.9");
        assert_eq!(proto_port, 34003);
    }

    #[tokio::test]
    async fn negotiate_reports_server_error() {
        let port = serve_once("UTNP/1.0 ERROR Unsupported protocol\n").await;
        let error = negotiate_protocol("127.0.0.1", "tmutp", port)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("ERROR Unsupported protocol"), "{error}");
    }

    #[tokio::test]
    async fn negotiate_works_with_ipv6_host() {
        let listener = match TcpListener::bind(("[::1]", 0)).await {
            Ok(listener) => listener,
            Err(_) => return,
        };
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 512];
            let _ = sock.read(&mut buf).await;
            sock.write_all(b"UTNP/1.0 START PROTOCOL tmutp 1.0 ::1 34005\n")
                .await
                .unwrap();
            sock.shutdown().await.unwrap();
        });
        let (host, proto_port) = negotiate_protocol("::1", "tmutp", port).await.unwrap();
        assert_eq!(host, "::1");
        assert_eq!(proto_port, 34005);
    }

    #[test]
    fn url_host_brackets_ipv6_only() {
        assert_eq!(url_host("::1"), "[::1]");
        assert_eq!(url_host("[::1]"), "[::1]");
        assert_eq!(url_host("example.com"), "example.com");
        assert_eq!(url_host("192.0.2.1"), "192.0.2.1");
    }
    #[tokio::test]
    async fn negotiate_rejects_newer_protocol_version() {
        let port = serve_once("UTNP/1.0 START PROTOCOL tmutp 2.0 10.0.0.7 100\n").await;
        let error = negotiate_protocol("127.0.0.1", "tmutp", port)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("supports only 1.0"), "{error}");
    }

    #[tokio::test]
    async fn negotiate_skips_newer_versions_in_available_body() {
        let port = serve_once(
            "UTNP/1.0 AVAILABLE\ntmutp 2.0 10.0.0.7 100\ntmutp 1.0 10.0.0.7 101\n",
        )
        .await;
        let (host, proto_port) = negotiate_protocol("127.0.0.1", "tmutp", port).await.unwrap();
        assert_eq!(host, "10.0.0.7");
        assert_eq!(proto_port, 101);
    }

    #[tokio::test]
    async fn negotiate_or_default_falls_back_on_newer_version() {
        let port = serve_once("UTNP/1.0 START PROTOCOL tmutp 2.0 10.0.0.7 100\n").await;
        let (host, proto_port) = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            negotiate_or_default("127.0.0.1", "tmutp", port, DEFAULT_TMUTP_PORT),
        )
        .await
        .unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(proto_port, DEFAULT_TMUTP_PORT);
    }

    #[tokio::test]
    async fn negotiate_or_default_falls_back_on_server_error() {
        let port = serve_once("UTNP/1.0 ERROR Unsupported protocol\n").await;
        let (host, proto_port) = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            negotiate_or_default("127.0.0.1", "tbip", port, DEFAULT_TBIP_PORT),
        )
        .await
        .unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(proto_port, DEFAULT_TBIP_PORT);
    }

}
