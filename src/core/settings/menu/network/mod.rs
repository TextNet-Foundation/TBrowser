
use crate::core::settings::{
    DefaultValue,
    def::{ItemDef, NodeDef, ParamDef, ParamKind, always_active},
};

// ─── Network ─────────────────────────────────────────────────────

static P_TIMEOUT: ParamDef = ParamDef {
    key: "timeout",
    label: "Timeout",
    description: "Timeout of network requests in seconds.",
    kind: ParamKind::Int,
    active: always_active,
    default: DefaultValue::Int(30),
};

static P_FOLLOW_REDIRECTS: ParamDef = ParamDef {
    key: "follow_redirects",
    label: "Follow redirects",
    description: "Automatically follow TMUTP redirects.",
    kind: ParamKind::Bool,
    active: always_active,
    default: DefaultValue::Bool(true),
};

static P_LEGACY_1: ParamDef = ParamDef {
    key: "allow_protocols_v1",
    label: "TNet Protocols v1 support",
    description: "Allow legacy TNet protocol version 1.0. Disabled by default because of its security limitations.",
    kind: ParamKind::Bool,
    active: always_active,
    default: DefaultValue::Bool(false),
};

// ─── Connection ───────────────────────────────────────────────────

static P_ALLOW_TCP: ParamDef = ParamDef {
    key: "allow_unencrypted_tcp",
    label: "Allow unencrypted TCP",
    description: "Allow connections without encryption as a fallback when a secure transport cannot be established. When disabled, unencrypted TCP is never offered.",
    kind: ParamKind::Bool,
    active: always_active,
    default: DefaultValue::Bool(false),
};

static P_SSH_TO_TLS: ParamDef = ParamDef {
    key: "ssh_to_tls",
    label: "SSH -> TLS fallback",
    description: "Offer a TLS fallback after an SSH connection or SSH key verification failure. TLS is still a protected transport.",
    kind: ParamKind::Bool,
    active: always_active,
    default: DefaultValue::Bool(false),
};

static P_FORCE_ENCRYPTION: ParamDef = ParamDef {
    key: "try_encryption_against_utnp",
    label: "Try encryption level against UTNP",
    description: "Attempt TLS or SSH even when UTNP reports that the selected transport is unsupported.",
    kind: ParamKind::Bool,
    active: always_active,
    default: DefaultValue::Bool(false),
};

const CONNECTION_CHILDREN: &[ItemDef] = &[
    ItemDef::Param(&P_ALLOW_TCP),
    ItemDef::Param(&P_SSH_TO_TLS),
    ItemDef::Param(&P_FORCE_ENCRYPTION),
];

static NODE_CONNECTION: NodeDef = NodeDef {
    key: "network.connection",
    label: "Connection",
    active: always_active,
    children: CONNECTION_CHILDREN,
};

// ─── SSH ──────────────────────────────────────────────────────────

static P_ASK_NEW_KEY: ParamDef = ParamDef {
    key: "ask_new_ssh_key",
    label: "Ask before trusting new keys",
    description: "Show a confirmation dialog when connecting to a server with an unknown SSH key. When disabled, the connection is dropped instead.",
    kind: ParamKind::Bool,
    active: always_active,
    default: DefaultValue::Bool(true),
};

static P_BLOCK_CHANGED_KEY: ParamDef = ParamDef {
    key: "block_changed_ssh_key",
    label: "Block changed SSH keys",
    description: "When enabled, a changed SSH key cannot be accepted interactively and the connection is dropped. When disabled, the user is asked whether to trust the new key.",
    kind: ParamKind::Bool,
    active: always_active,
    default: DefaultValue::Bool(false),
};

static P_SSH_STORE: ParamDef = ParamDef {
    key: "ssh_key_store",
    label: "SSH key store path",
    description: "Path to the local SSH known-hosts storage file.",
    kind: ParamKind::String,
    active: always_active,
    default: DefaultValue::Str("$XDG_CONFIG_HOME/tbrowser/ssh/known_hosts"),
};

const SSH_CHILDREN: &[ItemDef] = &[
    ItemDef::Param(&P_ASK_NEW_KEY),
    ItemDef::Param(&P_BLOCK_CHANGED_KEY),
    ItemDef::Param(&P_SSH_STORE),
];

static NODE_SSH: NodeDef = NodeDef {
    key: "network.ssh",
    label: "SSH & Trusted Keys",
    active: always_active,
    children: SSH_CHILDREN,
};

// ─── Root ─────────────────────────────────────────────────────────

const NETWORK_CHILDREN: &[ItemDef] = &[
    ItemDef::Param(&P_TIMEOUT),
    ItemDef::Param(&P_FOLLOW_REDIRECTS),
    ItemDef::Param(&P_LEGACY_1),
    ItemDef::Group(&NODE_CONNECTION),
    ItemDef::Group(&NODE_SSH),
];

pub static NODE_NETWORK: NodeDef = NodeDef {
    key: "network",
    label: "Network",
    active: always_active,
    children: NETWORK_CHILDREN,
};
