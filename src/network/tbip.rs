use anyhow::{Context, Result};
use async_trait::async_trait;
use url::Url;

use crate::network::base::{
    FetchOptions, FrameKind, ProgressCallback, ProtocolVersion, Response, TextProtocol,
};
use crate::network::negotiate::{self, Endpoint, Target};
use crate::network::protocol;
use crate::network::transport;

pub struct TbipClient;

#[async_trait]
impl TextProtocol for TbipClient {
    async fn fetch(
        &self,
        url: &str,
        progress: Option<&ProgressCallback>,
        options: &FetchOptions,
    ) -> anyhow::Result<Response> {
        let mut current = url.to_string();
        let mut remaining = options.max_redirects;

        loop {
            let target = Target::parse(&current)?;
            let endpoint = resolve_endpoint(&target, options).await?;

            let raw = send(&endpoint, &target, progress, options).await?;
            let wire = protocol::parse_response(&raw)?;

            if wire.code == 21 && options.follow_redirects && remaining > 0 {
                if let Some(location) = wire.headers.get("location").cloned() {
                    let base = Url::parse(&current).context("invalid redirect base URL")?;
                    current = base
                        .join(&location)
                        .with_context(|| format!("invalid redirect location: {location}"))?
                        .to_string();
                    remaining -= 1;
                    continue;
                }
            }

            let decoded = protocol::decode_body(&wire.headers, &wire.body)?;

            let mut headers = wire.headers;

            headers
                .entry("content-type".to_string())
                .or_insert_with(|| "application/octet-stream".to_string());

            let content_encoding = headers.get("content-encoding").cloned();
            let source_content_length =
                protocol::header_u64(&headers, "source-content-length");
            let file_sha256 = headers.get("file-sha256").cloned();

            return Ok(Response {
                status_line: wire.status_line,
                status_code: wire.code,
                status_name: wire.name,
                headers,
                body: decoded,
                text: String::new(),
                transport: endpoint.transport,
                version: endpoint.version,
                content_encoding,
                source_content_length,
                file_sha256,
            });
        }
    }
}

async fn send(
    endpoint: &Endpoint,
    target: &Target,
    progress: Option<&ProgressCallback>,
    options: &FetchOptions,
) -> Result<Vec<u8>> {
    let request = protocol::build_request(
        "TBIP",
        endpoint.version,
        "GET",
        &endpoint.host,
        &target.path_query,
        None,
        &[],
    );

    transport::exchange(
        &endpoint.host,
        endpoint.port,
        &request,
        FrameKind::Protocol,
        endpoint.transport,
        options.timeout_secs,
        options.ssh_key_store.clone(),
        progress,
    )
    .await
}

async fn resolve_endpoint(target: &Target, options: &FetchOptions) -> Result<Endpoint> {
    if let Some(port) = target.direct_port {
        return Ok(Endpoint {
            host: target.host.clone(),
            port,
            version: ProtocolVersion::V2,
            transport: target.transport,
        });
    }

    negotiate::negotiate(target, options, options.ssh_key_store.clone()).await
}
