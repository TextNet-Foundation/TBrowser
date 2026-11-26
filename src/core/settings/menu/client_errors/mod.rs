use crate::core::settings::{
    DefaultValue, Settings, Value,
    def::{ItemDef, NodeDef, ParamDef, ParamKind, always_active},
};

// ─── Error page definition ────────────────────────────────────────
//
// Each error page is represented by two parameters inside its category node:
//   <id>.mode            — Standard / Template / Custom
//   <id>.template_source — shown only when mode is Template or Custom
//
// The parameter key carries the error id ("net_001.mode"), so the full setting
// key stays "client_errors.<category>.<id>.mode" — identical to the previous
// per-error group layout. No migration is required.

macro_rules! error_page {
    (
        $ns:literal,
        $id:literal,
        $mode_param:ident,
        $source_param:ident,
        $label:literal,
        $description:literal
    ) => {
        static $mode_param: ParamDef = ParamDef {
            key: concat!($id, ".mode"),
            label: $label,
            description: $description,
            kind: ParamKind::Enum(&["Standard", "Template", "Custom"]),
            active: always_active,
            default: DefaultValue::EnumIndex(0),
        };

        static $source_param: ParamDef = ParamDef {
            key: concat!($id, ".template_source"),
            label: "Template file / Custom path",
            description: "For Template, enter a template filename from cetemplates/. For Custom, enter a path to a TMkUp template.",
            kind: ParamKind::String,
            active: |s: &Settings| {
                matches!(
                    s.get(concat!($ns, ".", $id, ".mode")),
                    Some(Value::EnumIndex(1 | 2))
                )
            },
            default: DefaultValue::Str(""),
        };
    };
}

// ─── Standard template ────────────────────────────────────────────

fn standard_source_active(settings: &Settings) -> bool {
    matches!(
        settings.get("client_errors.standard.mode"),
        Some(Value::EnumIndex(1 | 2))
    )
}

static P_STANDARD_MODE: ParamDef = ParamDef {
    key: "standard.mode",
    label: "Standard template",
    description: "Select the built-in standard_ce.tmkup, a template from cetemplates/, or a custom template path. This selection is inherited by every error configured as Standard.",
    kind: ParamKind::Enum(&["standard_ce.tmkup", "Template", "Custom"]),
    active: always_active,
    default: DefaultValue::EnumIndex(0),
};

static P_STANDARD_SOURCE: ParamDef = ParamDef {
    key: "standard.template_source",
    label: "Template file / Custom path",
    description: "For Template, enter a template filename from cetemplates/. For Custom, enter a path to a TMkUp template.",
    kind: ParamKind::String,
    active: standard_source_active,
    default: DefaultValue::Str(""),
};

// ─── Network errors ───────────────────────────────────────────────

error_page!(
    "client_errors.network", "net_001",
    P_NET_001_MODE, P_NET_001_SOURCE,
    "NET-001 — Connection failed",
    "Configure the page displayed when a connection cannot be established."
);

error_page!(
    "client_errors.network", "net_002",
    P_NET_002_MODE, P_NET_002_SOURCE,
    "NET-002 — TLS connection failed",
    "Configure the page displayed when TLS negotiation fails."
);

error_page!(
    "client_errors.network", "net_003",
    P_NET_003_MODE, P_NET_003_SOURCE,
    "NET-003 — SSH connection failed",
    "Configure the page displayed when SSH connection establishment fails."
);

error_page!(
    "client_errors.network", "net_004",
    P_NET_004_MODE, P_NET_004_SOURCE,
    "NET-004 — Unsupported transport",
    "Configure the page displayed when the selected transport is not supported by the server."
);

error_page!(
    "client_errors.network", "net_005",
    P_NET_005_MODE, P_NET_005_SOURCE,
    "NET-005 — Transport information unavailable",
    "Configure the page displayed when UTNP does not provide transport capability information."
);

error_page!(
    "client_errors.network", "net_006",
    P_NET_006_MODE, P_NET_006_SOURCE,
    "NET-006 — TCP connection failed",
    "Configure the page displayed when an explicitly permitted TCP connection fails."
);

error_page!(
    "client_errors.network", "net_007",
    P_NET_007_MODE, P_NET_007_SOURCE,
    "NET-007 — No secure transport available",
    "Configure the page displayed when no permitted secure transport can be established."
);

error_page!(
    "client_errors.network", "net_008",
    P_NET_008_MODE, P_NET_008_SOURCE,
    "NET-008 — Connection interrupted",
    "Configure the page displayed when an established connection is unexpectedly interrupted."
);

const NETWORK_ERRORS: &[ItemDef] = &[
    ItemDef::Param(&P_NET_001_MODE),
    ItemDef::Param(&P_NET_001_SOURCE),
    ItemDef::Param(&P_NET_002_MODE),
    ItemDef::Param(&P_NET_002_SOURCE),
    ItemDef::Param(&P_NET_003_MODE),
    ItemDef::Param(&P_NET_003_SOURCE),
    ItemDef::Param(&P_NET_004_MODE),
    ItemDef::Param(&P_NET_004_SOURCE),
    ItemDef::Param(&P_NET_005_MODE),
    ItemDef::Param(&P_NET_005_SOURCE),
    ItemDef::Param(&P_NET_006_MODE),
    ItemDef::Param(&P_NET_006_SOURCE),
    ItemDef::Param(&P_NET_007_MODE),
    ItemDef::Param(&P_NET_007_SOURCE),
    ItemDef::Param(&P_NET_008_MODE),
    ItemDef::Param(&P_NET_008_SOURCE),
];

static NODE_NETWORK_ERRORS: NodeDef = NodeDef {
    key: "client_errors.network",
    label: "Network errors",
    active: always_active,
    children: NETWORK_ERRORS,
};

// ─── Transport interaction errors ────────────────────────────────

error_page!(
    "client_errors.transport", "trn_001",
    P_TRN_001_MODE, P_TRN_001_SOURCE,
    "TRN-001 — SSH unsupported",
    "Configure the interactive page shown when the server reports that SSH is unsupported."
);

error_page!(
    "client_errors.transport", "trn_002",
    P_TRN_002_MODE, P_TRN_002_SOURCE,
    "TRN-002 — TLS unsupported",
    "Configure the interactive page shown when the server reports that TLS is unsupported."
);

error_page!(
    "client_errors.transport", "trn_003",
    P_TRN_003_MODE, P_TRN_003_SOURCE,
    "TRN-003 — SSH to TLS fallback",
    "Configure the interactive page shown when SSH fails and the SSH to TLS policy permits fallback."
);

error_page!(
    "client_errors.transport", "trn_004",
    P_TRN_004_MODE, P_TRN_004_SOURCE,
    "TRN-004 — TLS alternative transport",
    "Configure the interactive page shown when TLS fails and another permitted transport is available."
);

error_page!(
    "client_errors.transport", "trn_005",
    P_TRN_005_MODE, P_TRN_005_SOURCE,
    "TRN-005 — TCP only",
    "Configure the page displayed when the server only supports TCP."
);

error_page!(
    "client_errors.transport", "trn_006",
    P_TRN_006_MODE, P_TRN_006_SOURCE,
    "TRN-006 — No permitted transport",
    "Configure the page displayed when all server-supported transports are disabled by client policies."
);

const TRANSPORT_ERRORS: &[ItemDef] = &[
    ItemDef::Param(&P_TRN_001_MODE),
    ItemDef::Param(&P_TRN_001_SOURCE),
    ItemDef::Param(&P_TRN_002_MODE),
    ItemDef::Param(&P_TRN_002_SOURCE),
    ItemDef::Param(&P_TRN_003_MODE),
    ItemDef::Param(&P_TRN_003_SOURCE),
    ItemDef::Param(&P_TRN_004_MODE),
    ItemDef::Param(&P_TRN_004_SOURCE),
    ItemDef::Param(&P_TRN_005_MODE),
    ItemDef::Param(&P_TRN_005_SOURCE),
    ItemDef::Param(&P_TRN_006_MODE),
    ItemDef::Param(&P_TRN_006_SOURCE),
];

static NODE_TRANSPORT_ERRORS: NodeDef = NodeDef {
    key: "client_errors.transport",
    label: "Transport interaction",
    active: always_active,
    children: TRANSPORT_ERRORS,
};

// ─── SSH errors ───────────────────────────────────────────────────

error_page!(
    "client_errors.ssh", "ssh_001",
    P_SSH_001_MODE, P_SSH_001_SOURCE,
    "SSH-001 — Trust server key",
    "Configure the interactive page displayed when an unknown SSH key is encountered."
);

error_page!(
    "client_errors.ssh", "ssh_002",
    P_SSH_002_MODE, P_SSH_002_SOURCE,
    "SSH-002 — SSH key changed",
    "Configure the interactive page displayed when a server SSH key differs from the saved key."
);

error_page!(
    "client_errors.ssh", "ssh_003",
    P_SSH_003_MODE, P_SSH_003_SOURCE,
    "SSH-003 — Key change blocked",
    "Configure the page displayed when security policy prohibits accepting a changed SSH key."
);

error_page!(
    "client_errors.ssh", "ssh_004",
    P_SSH_004_MODE, P_SSH_004_SOURCE,
    "SSH-004 — Key verification failed",
    "Configure the page displayed when SSH key verification fails."
);

error_page!(
    "client_errors.ssh", "ssh_005",
    P_SSH_005_MODE, P_SSH_005_SOURCE,
    "SSH-005 — Cannot save key",
    "Configure the page displayed when the trusted SSH key cannot be saved."
);

error_page!(
    "client_errors.ssh", "ssh_006",
    P_SSH_006_MODE, P_SSH_006_SOURCE,
    "SSH-006 — Revoked SSH key",
    "Configure the page displayed when a server key is revoked."
);

const SSH_ERRORS: &[ItemDef] = &[
    ItemDef::Param(&P_SSH_001_MODE),
    ItemDef::Param(&P_SSH_001_SOURCE),
    ItemDef::Param(&P_SSH_002_MODE),
    ItemDef::Param(&P_SSH_002_SOURCE),
    ItemDef::Param(&P_SSH_003_MODE),
    ItemDef::Param(&P_SSH_003_SOURCE),
    ItemDef::Param(&P_SSH_004_MODE),
    ItemDef::Param(&P_SSH_004_SOURCE),
    ItemDef::Param(&P_SSH_005_MODE),
    ItemDef::Param(&P_SSH_005_SOURCE),
    ItemDef::Param(&P_SSH_006_MODE),
    ItemDef::Param(&P_SSH_006_SOURCE),
];

static NODE_SSH_ERRORS: NodeDef = NodeDef {
    key: "client_errors.ssh",
    label: "SSH & trust errors",
    active: always_active,
    children: SSH_ERRORS,
};

// ─── UTNP errors ──────────────────────────────────────────────────

error_page!(
    "client_errors.utnp", "utnp_001",
    P_UTNP_001_MODE, P_UTNP_001_SOURCE,
    "UTNP-001 — Unknown command",
    "Configure the page displayed for an unknown UTNP/2.0 command."
);

error_page!(
    "client_errors.utnp", "utnp_002",
    P_UTNP_002_MODE, P_UTNP_002_SOURCE,
    "UTNP-002 — Unsupported version",
    "Configure the page displayed when the server uses an unsupported UTNP version."
);

error_page!(
    "client_errors.utnp", "utnp_003",
    P_UTNP_003_MODE, P_UTNP_003_SOURCE,
    "UTNP-003 — Malformed response",
    "Configure the page displayed when a UTNP response is malformed."
);

error_page!(
    "client_errors.utnp", "utnp_004",
    P_UTNP_004_MODE, P_UTNP_004_SOURCE,
    "UTNP-004 — Negotiation failed",
    "Configure the page displayed when protocol negotiation fails."
);

error_page!(
    "client_errors.utnp", "utnp_005",
    P_UTNP_005_MODE, P_UTNP_005_SOURCE,
    "UTNP-005 — Protocol violation",
    "Configure the page displayed when a server response violates UTNP requirements."
);

error_page!(
    "client_errors.utnp", "utnp_006",
    P_UTNP_006_MODE, P_UTNP_006_SOURCE,
    "UTNP-006 — Server timeout",
    "Configure the page displayed when a UTNP request times out."
);

const UTNP_ERRORS: &[ItemDef] = &[
    ItemDef::Param(&P_UTNP_001_MODE),
    ItemDef::Param(&P_UTNP_001_SOURCE),
    ItemDef::Param(&P_UTNP_002_MODE),
    ItemDef::Param(&P_UTNP_002_SOURCE),
    ItemDef::Param(&P_UTNP_003_MODE),
    ItemDef::Param(&P_UTNP_003_SOURCE),
    ItemDef::Param(&P_UTNP_004_MODE),
    ItemDef::Param(&P_UTNP_004_SOURCE),
    ItemDef::Param(&P_UTNP_005_MODE),
    ItemDef::Param(&P_UTNP_005_SOURCE),
    ItemDef::Param(&P_UTNP_006_MODE),
    ItemDef::Param(&P_UTNP_006_SOURCE),
];

static NODE_UTNP_ERRORS: NodeDef = NodeDef {
    key: "client_errors.utnp",
    label: "UTNP errors",
    active: always_active,
    children: UTNP_ERRORS,
};

// ─── URL errors ───────────────────────────────────────────────────

error_page!(
    "client_errors.addressing", "url_001",
    P_URL_001_MODE, P_URL_001_SOURCE,
    "URL-001 — Invalid address",
    "Configure the page displayed for an invalid URL."
);

error_page!(
    "client_errors.addressing", "url_002",
    P_URL_002_MODE, P_URL_002_SOURCE,
    "URL-002 — Unknown scheme",
    "Configure the page displayed for an unknown URL scheme."
);

error_page!(
    "client_errors.addressing", "url_003",
    P_URL_003_MODE, P_URL_003_SOURCE,
    "URL-003 — Invalid port",
    "Configure the page displayed for an invalid port."
);

error_page!(
    "client_errors.addressing", "url_004",
    P_URL_004_MODE, P_URL_004_SOURCE,
    "URL-004 — Unknown special port",
    "Configure the page displayed for an unknown special port."
);

error_page!(
    "client_errors.addressing", "url_005",
    P_URL_005_MODE, P_URL_005_SOURCE,
    "URL-005 — Invalid UTNP address",
    "Configure the page displayed for an address that cannot be used with UTNP."
);

error_page!(
    "client_errors.addressing", "url_006",
    P_URL_006_MODE, P_URL_006_SOURCE,
    "URL-006 — Host not found",
    "Configure the page displayed when the requested host cannot be resolved."
);

const ADDRESSING_ERRORS: &[ItemDef] = &[
    ItemDef::Param(&P_URL_001_MODE),
    ItemDef::Param(&P_URL_001_SOURCE),
    ItemDef::Param(&P_URL_002_MODE),
    ItemDef::Param(&P_URL_002_SOURCE),
    ItemDef::Param(&P_URL_003_MODE),
    ItemDef::Param(&P_URL_003_SOURCE),
    ItemDef::Param(&P_URL_004_MODE),
    ItemDef::Param(&P_URL_004_SOURCE),
    ItemDef::Param(&P_URL_005_MODE),
    ItemDef::Param(&P_URL_005_SOURCE),
    ItemDef::Param(&P_URL_006_MODE),
    ItemDef::Param(&P_URL_006_SOURCE),
];

static NODE_ADDRESSING_ERRORS: NodeDef = NodeDef {
    key: "client_errors.addressing",
    label: "Addressing errors",
    active: always_active,
    children: ADDRESSING_ERRORS,
};

// ─── TMUTP / TBIP errors ───────────────────────────────────────────

error_page!(
    "client_errors.protocols", "proto_001",
    P_PROTO_001_MODE, P_PROTO_001_SOURCE,
    "PROTO-001 — Unsupported TMUTP version",
    "Configure the page displayed for an unsupported TMUTP version."
);

error_page!(
    "client_errors.protocols", "proto_002",
    P_PROTO_002_MODE, P_PROTO_002_SOURCE,
    "PROTO-002 — Malformed TMUTP response",
    "Configure the page displayed for a malformed TMUTP response."
);

error_page!(
    "client_errors.protocols", "proto_003",
    P_PROTO_003_MODE, P_PROTO_003_SOURCE,
    "PROTO-003 — Unknown TMUTP command",
    "Configure the page displayed for an unknown TMUTP command."
);

error_page!(
    "client_errors.protocols", "proto_004",
    P_PROTO_004_MODE, P_PROTO_004_SOURCE,
    "PROTO-004 — Unsupported TBIP version",
    "Configure the page displayed for an unsupported TBIP version."
);

error_page!(
    "client_errors.protocols", "proto_005",
    P_PROTO_005_MODE, P_PROTO_005_SOURCE,
    "PROTO-005 — Malformed TBIP response",
    "Configure the page displayed for a malformed TBIP response."
);

error_page!(
    "client_errors.protocols", "proto_006",
    P_PROTO_006_MODE, P_PROTO_006_SOURCE,
    "PROTO-006 — Redirect failed",
    "Configure the page displayed when a redirect cannot be processed."
);

error_page!(
    "client_errors.protocols", "proto_007",
    P_PROTO_007_MODE, P_PROTO_007_SOURCE,
    "PROTO-007 — Too many redirects",
    "Configure the page displayed when the redirect limit is exceeded."
);

const PROTOCOL_ERRORS: &[ItemDef] = &[
    ItemDef::Param(&P_PROTO_001_MODE),
    ItemDef::Param(&P_PROTO_001_SOURCE),
    ItemDef::Param(&P_PROTO_002_MODE),
    ItemDef::Param(&P_PROTO_002_SOURCE),
    ItemDef::Param(&P_PROTO_003_MODE),
    ItemDef::Param(&P_PROTO_003_SOURCE),
    ItemDef::Param(&P_PROTO_004_MODE),
    ItemDef::Param(&P_PROTO_004_SOURCE),
    ItemDef::Param(&P_PROTO_005_MODE),
    ItemDef::Param(&P_PROTO_005_SOURCE),
    ItemDef::Param(&P_PROTO_006_MODE),
    ItemDef::Param(&P_PROTO_006_SOURCE),
    ItemDef::Param(&P_PROTO_007_MODE),
    ItemDef::Param(&P_PROTO_007_SOURCE),
];

static NODE_PROTOCOL_ERRORS: NodeDef = NodeDef {
    key: "client_errors.protocols",
    label: "TMUTP & TBIP errors",
    active: always_active,
    children: PROTOCOL_ERRORS,
};

// ─── Security errors ──────────────────────────────────────────────

error_page!(
    "client_errors.security", "sec_001",
    P_SEC_001_MODE, P_SEC_001_SOURCE,
    "SEC-001 — Blocked by security policy",
    "Configure the page displayed when a connection is blocked by a security policy."
);

error_page!(
    "client_errors.security", "sec_002",
    P_SEC_002_MODE, P_SEC_002_SOURCE,
    "SEC-002 — Unencrypted TCP blocked",
    "Configure the page displayed when unencrypted TCP is prohibited."
);

error_page!(
    "client_errors.security", "sec_003",
    P_SEC_003_MODE, P_SEC_003_SOURCE,
    "SEC-003 — Protocol version disabled",
    "Configure the page displayed when the requested protocol version is disabled."
);

error_page!(
    "client_errors.security", "sec_004",
    P_SEC_004_MODE, P_SEC_004_SOURCE,
    "SEC-004 — Transport disabled",
    "Configure the page displayed when the selected transport is disabled in settings."
);

error_page!(
    "client_errors.security", "sec_005",
    P_SEC_005_MODE, P_SEC_005_SOURCE,
    "SEC-005 — Server requests insecure mode",
    "Configure the page displayed when the server requests an insecure connection mode."
);

error_page!(
    "client_errors.security", "sec_006",
    P_SEC_006_MODE, P_SEC_006_SOURCE,
    "SEC-006 — Security restrictions",
    "Configure the page displayed when security restrictions prevent continuing."
);

const SECURITY_ERRORS: &[ItemDef] = &[
    ItemDef::Param(&P_SEC_001_MODE),
    ItemDef::Param(&P_SEC_001_SOURCE),
    ItemDef::Param(&P_SEC_002_MODE),
    ItemDef::Param(&P_SEC_002_SOURCE),
    ItemDef::Param(&P_SEC_003_MODE),
    ItemDef::Param(&P_SEC_003_SOURCE),
    ItemDef::Param(&P_SEC_004_MODE),
    ItemDef::Param(&P_SEC_004_SOURCE),
    ItemDef::Param(&P_SEC_005_MODE),
    ItemDef::Param(&P_SEC_005_SOURCE),
    ItemDef::Param(&P_SEC_006_MODE),
    ItemDef::Param(&P_SEC_006_SOURCE),
];

static NODE_SECURITY_ERRORS: NodeDef = NodeDef {
    key: "client_errors.security",
    label: "Security errors",
    active: always_active,
    children: SECURITY_ERRORS,
};

// ─── Client Errors root ───────────────────────────────────────────

const CLIENT_ERRORS_CHILDREN: &[ItemDef] = &[
    ItemDef::Param(&P_STANDARD_MODE),
    ItemDef::Param(&P_STANDARD_SOURCE),
    ItemDef::Group(&NODE_NETWORK_ERRORS),
    ItemDef::Group(&NODE_TRANSPORT_ERRORS),
    ItemDef::Group(&NODE_SSH_ERRORS),
    ItemDef::Group(&NODE_UTNP_ERRORS),
    ItemDef::Group(&NODE_ADDRESSING_ERRORS),
    ItemDef::Group(&NODE_PROTOCOL_ERRORS),
    ItemDef::Group(&NODE_SECURITY_ERRORS),
];

pub static NODE_CLIENT_ERRORS: NodeDef = NodeDef {
    key: "client_errors",
    label: "Client Errors",
    active: always_active,
    children: CLIENT_ERRORS_CHILDREN,
};
