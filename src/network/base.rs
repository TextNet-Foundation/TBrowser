use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Tcp,
    Tls,
    Ssh,
}

impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Transport::Tcp => "TCP",
            Transport::Tls => "TLS",
            Transport::Ssh => "SSH",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolVersion {
    V1,
    V2,
}

impl ProtocolVersion {
    pub fn number(self) -> &'static str {
        match self {
            ProtocolVersion::V1 => "1.0",
            ProtocolVersion::V2 => "2.0",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FrameKind {
    Protocol,
    Utnp,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Response {
    pub status_line: String,
    pub status_code: u16,
    pub status_name: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub text: String,
    pub transport: Transport,
    pub version: ProtocolVersion,
    pub content_encoding: Option<String>,
    pub source_content_length: Option<u64>,
    pub file_sha256: Option<String>,
}

impl Response {
    pub fn ok(&self) -> bool {
        matches!(self.status_code, 20 | 22)
    }

    #[allow(dead_code)]
    pub fn moved(&self) -> bool {
        self.status_code == 21
    }
}

pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct FetchOptions {
    pub follow_redirects: bool,
    pub max_redirects: u32,
    pub timeout_secs: u64,
    pub allow_v1: bool,
    #[allow(dead_code)]
    pub allow_unencrypted_tcp: bool,
    pub try_encryption_against_utnp: bool,
    pub ssh_key_store: Option<std::path::PathBuf>,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            follow_redirects: true,
            max_redirects: 10,
            timeout_secs: 30,
            allow_v1: false,
            allow_unencrypted_tcp: false,
            try_encryption_against_utnp: false,
            ssh_key_store: None,
        }
    }
}

#[async_trait]
pub trait TextProtocol: Send + Sync {
    async fn fetch(
        &self,
        url: &str,
        progress: Option<&ProgressCallback>,
        options: &FetchOptions,
    ) -> Result<Response>;
}

#[async_trait]
pub trait PostCompatibleProtocol: TextProtocol {
    #[allow(unused)]
    async fn post(
        &self,
        url: &str,
        data: &[(&str, &str)],
        progress: Option<&ProgressCallback>,
        options: &FetchOptions,
    ) -> Result<Response>;
}
