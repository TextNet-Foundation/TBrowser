use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use bytes::Bytes;
use russh::client;
use russh::keys::known_hosts::check_known_hosts_path;
use russh::keys::PublicKeyOrCertificate;
use russh::ChannelMsg;
use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

use super::base::{FrameKind, ProgressCallback, Transport};

const MAX_RESPONSE_HEADER: usize = 64 * 1024;
const MAX_RESPONSE_BODY: usize = 256 * 1024 * 1024;

#[derive(Debug)]
pub enum TlsAttemptError {
    Unavailable(String),
    Fatal(anyhow::Error),
}

impl TlsAttemptError {
    pub fn into_anyhow(self) -> anyhow::Error {
        match self {
            TlsAttemptError::Unavailable(message) => {
                anyhow!("TLS unavailable: {message}")
            }
            TlsAttemptError::Fatal(error) => error,
        }
    }
}

#[derive(Debug)]
pub enum SshAttemptError {
    Unavailable(String),
    Untrusted(String),
    Failed(anyhow::Error),
}

impl SshAttemptError {
    pub fn into_anyhow(self) -> anyhow::Error {
        match self {
            SshAttemptError::Unavailable(message) => {
                anyhow!("SSH unavailable: {message}")
            }
            SshAttemptError::Untrusted(message) => {
                anyhow!("SSH host key is not trusted: {message}")
            }
            SshAttemptError::Failed(error) => error,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn exchange(
    host: &str,
    port: u16,
    request: &[u8],
    frame: FrameKind,
    transport: Transport,
    timeout_secs: u64,
    ssh_key_store: Option<PathBuf>,
    progress: Option<&ProgressCallback>,
) -> Result<Vec<u8>> {
    let duration = Duration::from_secs(timeout_secs.max(1));

    match transport {
        Transport::Tcp => {
            exchange_tcp(host, port, request, frame, duration, progress).await
        }
        Transport::Tls => exchange_tls(host, port, request, frame, duration, progress)
            .await
            .map_err(TlsAttemptError::into_anyhow),
        Transport::Ssh => exchange_ssh(
            host,
            port,
            request,
            frame,
            duration,
            ssh_key_store,
            progress,
        )
        .await
        .map_err(SshAttemptError::into_anyhow),
    }
}

/// Exchanges a UTNP request, falling back from the requested transport.
///
/// UTNP endpoints commonly speak plain TCP even for protocols whose data
/// port is encrypted, so a failed TLS handshake (or an unavailable SSH
/// transport) retries the negotiation over TCP. SSH host-key problems are
/// never downgraded: they surface as-is and require explicit user action.
pub async fn exchange_negotiation(
    host: &str,
    port: u16,
    request: &[u8],
    timeout_secs: u64,
    ssh_key_store: Option<PathBuf>,
    preferred: Transport,
) -> Result<Vec<u8>> {
    let duration = Duration::from_secs(timeout_secs.max(1));

    match preferred {
        Transport::Tcp => {
            exchange_tcp(host, port, request, FrameKind::Utnp, duration, None).await
        }

        Transport::Tls => match exchange_tls(host, port, request, FrameKind::Utnp, duration, None)
            .await
        {
            Ok(response) => Ok(response),

            Err(TlsAttemptError::Unavailable(message)) => {
                exchange_tcp(host, port, request, FrameKind::Utnp, duration, None)
                    .await
                    .with_context(|| {
                        format!("TLS negotiation failed ({message}) and TCP fallback failed")
                    })
            }

            Err(TlsAttemptError::Fatal(error)) => Err(error),
        },

        Transport::Ssh => match exchange_ssh(
            host,
            port,
            request,
            FrameKind::Utnp,
            duration,
            ssh_key_store.clone(),
            None,
        )
        .await
        {
            Ok(response) => Ok(response),

            Err(SshAttemptError::Untrusted(message)) => {
                bail!("SSH host key is not trusted: {message}")
            }

            Err(SshAttemptError::Failed(error)) => Err(error),

            Err(SshAttemptError::Unavailable(message)) => {
                match exchange_tls(host, port, request, FrameKind::Utnp, duration, None).await {
                    Ok(response) => Ok(response),

                    Err(TlsAttemptError::Unavailable(tls_message)) => {
                        exchange_tcp(host, port, request, FrameKind::Utnp, duration, None)
                            .await
                            .with_context(|| {
                                format!(
                                    "SSH unavailable ({message}), TLS unavailable ({tls_message}), TCP fallback failed"
                                )
                            })
                    }

                    Err(TlsAttemptError::Fatal(error)) => Err(error),
                }
            }
        },
    }
}

async fn exchange_tcp(
    host: &str,
    port: u16,
    request: &[u8],
    frame: FrameKind,
    duration: Duration,
    progress: Option<&ProgressCallback>,
) -> Result<Vec<u8>> {
    let stream = timeout(duration, TcpStream::connect((host, port)))
        .await
        .map_err(|_| anyhow!("TCP connection to {host}:{port} timed out"))?
        .with_context(|| format!("Cannot connect to {host}:{port}"))?;

    stream.set_nodelay(true)?;

    send_and_receive(stream, request, frame, duration, progress).await
}

async fn exchange_tls(
    host: &str,
    port: u16,
    request: &[u8],
    frame: FrameKind,
    duration: Duration,
    progress: Option<&ProgressCallback>,
) -> std::result::Result<Vec<u8>, TlsAttemptError> {
    let mut roots = RootCertStore::empty();

    let native_certs = rustls_native_certs::load_native_certs();

    for error in &native_certs.errors {
        eprintln!("Warning: failed to load a system root certificate: {error}");
    }

    for cert in native_certs.certs {
        roots
            .add(cert)
            .map_err(anyhow::Error::from)
            .map_err(TlsAttemptError::Fatal)?;
    }

    if roots.is_empty() {
        return Err(TlsAttemptError::Fatal(anyhow!(
            "No trusted CA certificates found in the system store"
        )));
    }

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));

    let stream = timeout(duration, TcpStream::connect((host, port)))
        .await
        .map_err(|_| {
            TlsAttemptError::Unavailable(format!(
                "TCP connection to {host}:{port} timed out"
            ))
        })?
        .map_err(|error| TlsAttemptError::Unavailable(error.to_string()))?;

    stream
        .set_nodelay(true)
        .map_err(|error| TlsAttemptError::Fatal(error.into()))?;

    let server_name = ServerName::try_from(host.to_string())
        .map_err(|error| TlsAttemptError::Fatal(error.into()))?;

    let tls_stream = match timeout(duration, connector.connect(server_name, stream)).await {
        Ok(Ok(stream)) => stream,

        Ok(Err(error)) => {
            return match error.kind() {
                std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::BrokenPipe => {
                    Err(TlsAttemptError::Unavailable(error.to_string()))
                }

                _ => Err(TlsAttemptError::Fatal(anyhow!(
                    "TLS handshake or certificate verification failed: {error}"
                ))),
            };
        }

        Err(_) => {
            return Err(TlsAttemptError::Unavailable(
                "TLS handshake timed out".to_string(),
            ));
        }
    };

    send_and_receive(tls_stream, request, frame, duration, progress)
        .await
        .map_err(TlsAttemptError::Fatal)
}

async fn send_and_receive<S>(
    mut stream: S,
    request: &[u8],
    frame: FrameKind,
    duration: Duration,
    progress: Option<&ProgressCallback>,
) -> Result<Vec<u8>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.write_all(request).await?;
    stream.flush().await?;

    match frame {
        FrameKind::Protocol => read_protocol_response(stream, duration, progress).await,

        FrameKind::Utnp => {
            stream.shutdown().await?;

            let mut response = Vec::new();

            timeout(duration, stream.read_to_end(&mut response))
                .await
                .map_err(|_| anyhow!("Timed out waiting for UTNP response"))??;

            if response.is_empty() {
                bail!("UTNP server returned an empty response");
            }

            Ok(response)
        }
    }
}

async fn read_protocol_response<S>(
    stream: S,
    duration: Duration,
    progress: Option<&ProgressCallback>,
) -> Result<Vec<u8>>
where
    S: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(stream);

    let mut header = Vec::new();

    timeout(duration, async {
        loop {
            let mut line = Vec::new();
            let count = reader.read_until(b'\n', &mut line).await?;

            if count == 0 {
                bail!("Connection closed before response headers were complete");
            }

            header.extend_from_slice(&line);

            if header.len() > MAX_RESPONSE_HEADER {
                bail!("Response headers exceed the size limit");
            }

            if line == b"\n" || line == b"\r\n" {
                break;
            }
        }

        Ok::<(), anyhow::Error>(())
    })
    .await
    .map_err(|_| anyhow!("Timed out waiting for response headers"))??;

    let content_length = parse_content_length(&header)?;

    if content_length > MAX_RESPONSE_BODY {
        bail!("Response body exceeds the size limit");
    }

    let mut body = vec![0u8; content_length];
    let mut total_read = 0usize;

    while total_read < content_length {
        let n = reader.read(&mut body[total_read..]).await?;

        if n == 0 {
            bail!("Connection closed before the response body was complete");
        }

        total_read += n;

        if let Some(callback) = progress {
            callback(total_read as u64, content_length as u64);
        }
    }

    header.extend_from_slice(&body);

    Ok(header)
}

pub fn parse_content_length(header: &[u8]) -> Result<usize> {
    let text = std::str::from_utf8(header)
        .context("Response headers are not valid UTF-8")?;

    for line in text.lines().skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                return value
                    .trim()
                    .parse::<usize>()
                    .context("Invalid Content-Length in response");
            }
        }
    }

    bail!("Response does not contain Content-Length")
}

pub fn response_header_end(response: &[u8]) -> Option<usize> {
    response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
        .or_else(|| {
            response
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| position + 2)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyStatus {
    Unknown,
    Trusted,
    Changed,
}

struct KnownHostsHandler {
    host: String,
    port: u16,
    path: Option<PathBuf>,
    status: Arc<Mutex<KeyStatus>>,
}

impl client::Handler for KnownHostsHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        let public_key = server_public_key.public_key();

        let status = match &self.path {
            Some(path) => {
                match check_known_hosts_path(&self.host, self.port, &public_key, path) {
                    Ok(true) => KeyStatus::Trusted,
                    Ok(false) => KeyStatus::Unknown,
                    Err(_) => KeyStatus::Changed,
                }
            }
            None => KeyStatus::Unknown,
        };

        *self.status.lock().unwrap() = status;

        Ok(matches!(status, KeyStatus::Trusted))
    }
}

async fn exchange_ssh(
    host: &str,
    port: u16,
    request: &[u8],
    frame: FrameKind,
    duration: Duration,
    ssh_key_store: Option<PathBuf>,
    progress: Option<&ProgressCallback>,
) -> std::result::Result<Vec<u8>, SshAttemptError> {
    let status = Arc::new(Mutex::new(KeyStatus::Unknown));

    let handler = KnownHostsHandler {
        host: host.to_string(),
        port,
        path: ssh_key_store,
        status: status.clone(),
    };

    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        ..Default::default()
    });

    let mut session = match timeout(duration, client::connect(config, (host, port), handler)).await {
        Ok(Ok(session)) => session,

        Ok(Err(error)) => {
            return Err(classify_ssh_connect_error(&status, error));
        }

        Err(_) => {
            let status = *status.lock().unwrap();

            return Err(match status {
                KeyStatus::Trusted => SshAttemptError::Failed(anyhow!(
                    "SSH connection timed out after host-key verification"
                )),
                KeyStatus::Unknown | KeyStatus::Changed => SshAttemptError::Unavailable(
                    "SSH connection timed out".to_string(),
                ),
            });
        }
    };

    let auth = timeout(duration, session.authenticate_none("tbrowser"))
        .await
        .map_err(|_| {
            SshAttemptError::Failed(anyhow!("SSH authentication timed out"))
        })?
        .map_err(|error| {
            SshAttemptError::Failed(anyhow!("SSH authentication failed: {error}"))
        })?;

    if !auth.success() {
        return Err(SshAttemptError::Failed(anyhow!(
            "SSH server rejected none authentication"
        )));
    }

    let mut channel = timeout(duration, session.channel_open_session())
        .await
        .map_err(|_| {
            SshAttemptError::Failed(anyhow!("SSH channel open timed out"))
        })?
        .map_err(|error| {
            SshAttemptError::Failed(anyhow!(
                "Cannot open SSH session channel: {error}"
            ))
        })?;

    channel
        .request_shell(true)
        .await
        .map_err(|error| {
            SshAttemptError::Failed(anyhow!(
                "SSH protocol channel was rejected: {error}"
            ))
        })?;

    channel
        .data_bytes(Bytes::from(request.to_vec()))
        .await
        .map_err(|error| {
            SshAttemptError::Failed(anyhow!(
                "Cannot send request over SSH: {error}"
            ))
        })?;

    channel.eof().await.map_err(|error| {
        SshAttemptError::Failed(anyhow!("Cannot finish SSH request: {error}"))
    })?;

    let mut response = Vec::new();

    loop {
        let wait_time = match frame {
            FrameKind::Protocol => duration,
            FrameKind::Utnp if response.is_empty() => duration,
            FrameKind::Utnp => Duration::from_secs(1),
        };

        let message = match timeout(wait_time, channel.wait()).await {
            Ok(message) => message,

            Err(_) if matches!(frame, FrameKind::Utnp) && !response.is_empty() => {
                break;
            }

            Err(_) => {
                return Err(SshAttemptError::Failed(anyhow!(
                    "Timed out waiting for SSH response"
                )));
            }
        };

        match message {
            Some(ChannelMsg::Data { data }) => {
                response.extend_from_slice(&data);
            }

            Some(ChannelMsg::ExtendedData { data, .. }) => {
                response.extend_from_slice(&data);
            }

            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                break;
            }

            Some(_) => {}
        }

        if let FrameKind::Protocol = frame {
            if let Some(expected) = expected_response_length(&response)
                .map_err(SshAttemptError::Failed)?
            {
                if let Some(callback) = progress {
                    callback(response.len() as u64, expected as u64);
                }

                if response.len() >= expected {
                    break;
                }
            }
        }
    }

    if response.is_empty() {
        return Err(SshAttemptError::Failed(anyhow!(
            "SSH server returned an empty response"
        )));
    }

    let _ = channel.close().await;

    Ok(response)
}

fn classify_ssh_connect_error(
    status: &Arc<Mutex<KeyStatus>>,
    error: russh::Error,
) -> SshAttemptError {
    match *status.lock().unwrap() {
        KeyStatus::Changed => SshAttemptError::Untrusted(
            "The server key is unknown or has changed".to_string(),
        ),
        KeyStatus::Unknown => SshAttemptError::Untrusted(
            "The server key is not present in the SSH key store".to_string(),
        ),
        KeyStatus::Trusted => SshAttemptError::Failed(anyhow!(
            "SSH connection failed after host-key verification: {error}"
        )),
    }
}

fn expected_response_length(response: &[u8]) -> Result<Option<usize>> {
    let Some(header_end) = response_header_end(response) else {
        return Ok(None);
    };

    let content_length = parse_content_length(&response[..header_end])?;

    if content_length > MAX_RESPONSE_BODY {
        bail!("Response body exceeds the size limit");
    }

    Ok(Some(header_end + content_length))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    async fn plain_utnp_server() -> (u16, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = tokio::spawn(async move {
            let (mut first, _) = listener.accept().await.unwrap();
            let mut hello = [0u8; 1024];
            let _ = timeout(
                Duration::from_secs(5),
                first.read(&mut hello),
            )
            .await;
            drop(first);

            let (mut second, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let _ = second.read_to_end(&mut request).await;
            second
                .write_all(
                    b"UTNP/2.0 START PROTOCOL tmutp 2.0 127.0.0.1 100\nSecurity: ssh\n",
                )
                .await
                .unwrap();
            let _ = second.shutdown().await;
            request
        });

        (port, handle)
    }

    #[tokio::test]
    async fn negotiation_falls_back_to_tcp_when_tls_is_unavailable() {
        let (port, server) = plain_utnp_server().await;

        let response = exchange_negotiation(
            "127.0.0.1",
            port,
            b"UTNP/2.0 NEGOTIATE tmutp\nSupported-Versions: 2.0,1.0\n\n",
            5,
            None,
            Transport::Tls,
        )
        .await
        .expect("negotiation must fall back to plain TCP");

        assert!(String::from_utf8_lossy(&response).starts_with("UTNP/2.0 START"));

        let request = server.await.unwrap();
        assert!(
            String::from_utf8_lossy(&request).contains("NEGOTIATE tmutp"),
            "fallback request must reach the server"
        );
    }
}
