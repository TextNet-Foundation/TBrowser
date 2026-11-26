use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use url::Url;

use super::base::{FetchOptions, FrameKind};
use super::negotiate::DEFAULT_UTNP_PORT;
use super::transport;

#[allow(unused)]
pub async fn list(
    host: &str,
    utnp_port: u16,
    options: &FetchOptions,
    ssh_key_store: Option<PathBuf>,
) -> Result<String> {
    send_command(host, utnp_port, "LIST", options, ssh_key_store).await
}

#[allow(unused)]
pub async fn explore(
    host: &str,
    utnp_port: u16,
    options: &FetchOptions,
    ssh_key_store: Option<PathBuf>,
) -> Result<String> {
    send_command(host, utnp_port, "EXPLORE", options, ssh_key_store).await
}

#[allow(unused)]
pub async fn fetch_url(url: &str, options: &FetchOptions) -> Result<String> {
    let parsed = Url::parse(url).context("invalid UTNP URL")?;
    let host = parsed.host_str().unwrap_or("localhost").to_string();
    let port = parsed.port().unwrap_or(DEFAULT_UTNP_PORT);
    let path = parsed.path().trim_start_matches('/');

    let (command, arg) = match path.split_once('/') {
        Some((command, arg)) => (command.to_ascii_uppercase(), Some(arg.to_string())),
        None => (path.to_ascii_uppercase(), None),
    };

    match command.as_str() {
        "LIST" => list(&host, port, options, None).await,
        "EXPLORE" => explore(&host, port, options, None).await,
        "NEGOTIATE" => {
            let Some(protocol) = arg else {
                bail!("negotiate requires a protocol name");
            };

            let protocol = match protocol.to_ascii_lowercase().as_str() {
                "tmutp" => "tmutp",
                "tbip" => "tbip",
                other => bail!("Unknown protocol: {other}"),
            };

            let target = super::negotiate::Target {
                protocol,
                host,
                path_query: "/".to_string(),
                direct_port: None,
                utnp_port: Some(port),
                transport: super::base::Transport::Tls,
            };

            let endpoint = super::negotiate::negotiate(&target, options, None).await?;

            Ok(format!(
                "UTNP START PROTOCOL {} {} {} {}",
                protocol,
                endpoint.version.number(),
                endpoint.host,
                endpoint.port
            ))
        }
        other => bail!("Unknown UTNP command: {other}"),
    }
}

async fn send_command(
    host: &str,
    utnp_port: u16,
    command: &str,
    options: &FetchOptions,
    ssh_key_store: Option<PathBuf>,
) -> Result<String> {
    let request = format!("UTNP/2.0 {command}\n\n");

    let response = transport::exchange(
        host,
        utnp_port,
        request.as_bytes(),
        FrameKind::Utnp,
        super::base::Transport::Tls,
        options.timeout_secs,
        ssh_key_store,
        None,
    )
    .await?;

    Ok(String::from_utf8_lossy(&response).into_owned())
}
