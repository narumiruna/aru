# MCP Registry and Agent Capability Spike

Date: 2026-07-30

## Scope

This bounded spike fixes the aru v1 behavior for the preview MCP Registry, Codex project MCP, and Claude Code project MCP. Evidence is limited to the checked-out OpenAPI/schema snapshots under `third_party/reference/`, the current official documentation, local fixtures, and read-only requests to the public registry. The Registry remains preview, so decoding stays isolated in `src/registry.rs` and fails closed.

## Registry observations

Read-only requests to `https://registry.modelcontextprotocol.io` confirmed:

- `GET /v0.1/servers?limit=2` returned two records and the opaque cursor `ac.inference.sh/mcp:1.0.1`.
- Exact lookup for `ac.inference.sh/mcp` version `1.0.0` returned HTTP 200, active status, no packages, and two remotes.
- `/v0.1/servers/ac.inference.sh%2Fmcp/versions` returned four versions newest-first and no cursor for that inventory.

The local OpenAPI contract additionally establishes:

- cursors are opaque and must be replayed verbatim;
- exact version and special `latest` share one endpoint;
- version strings may be non-SemVer;
- lifecycle status is `active`, `deprecated`, or `deleted`;
- one version may contain multiple packages and remotes, with no candidate preference implied by array order.

### Decisions

- Follow pagination with a 100-page, 10,000-record ceiling; reject repeated cursors.
- Cap each decoded response at 10 MiB and reject non-success status, malformed JSON, or schema mismatch. Never treat an error as an empty inventory.
- Resolve SemVer requirements over active versions. A non-SemVer version is accepted only as `=<literal>` and uses exact lookup.
- Normalize and hash only the selected server id, exact version, and selected candidate fields.
- Sort candidates before diagnostics, but require exactly one after manifest selectors and capability filtering.
- Support npm packages that can be rendered as the argument array `npx [runtime arguments] --yes <identifier>@<exact-version> [package arguments]`. PyPI, Cargo, OCI, NuGet, MCPB, unknown runtimes, unresolved argument templates, and fixed environment values fail closed in v1.
- Secret stdio environment inputs are retained only as environment variable names. No secret value is read or locked.
- Support static HTTPS streamable-HTTP remotes. A header is portable only when it is an exact `{ENV_NAME}` reference, or `Authorization: Bearer {ENV_NAME}`; all other variable/header templates fail closed.

Fixtures under `tests/fixtures/registry/` cover pagination, active/deprecated and SemVer/non-SemVer records, npm package metadata, remote bearer references, and ambiguous candidates.

## Agent capability observations

| Capability | Codex project `.codex/config.toml` | Claude Code project `.mcp.json` | aru v1 |
| --- | --- | --- | --- |
| stdio command + argument array | Supported | Supported | Supported |
| stdio environment by variable name | `env_vars` | `env.NAME = "${NAME}"` | Supported |
| streamable HTTP | `url` | `type = "http"`, `url` | Supported |
| SSE | Not a distinct current Codex transport | Supported but deprecated | Rejected |
| bearer token by environment name | `bearer_token_env_var` | `Authorization: Bearer ${NAME}` | Supported |
| arbitrary header by environment name | `env_http_headers` | `headers.NAME = "${ENV}"` | Supported |
| inline bearer/secret value | Unsafe | Representable but unsafe | Rejected |
| OAuth credential storage | Host-managed | Host-managed | Not managed by aru |

Evidence:

- Codex official MCP documentation and `third_party/reference/codex/codex-rs/config/src/mcp_types.rs` define project `.codex/config.toml`, stdio `env_vars`, streamable HTTP `bearer_token_env_var`, and `env_http_headers`.
- Claude Code official MCP documentation documents project `.mcp.json`, stdio/HTTP/SSE transports, and `${VAR}` expansion in `command`, `args`, `env`, `url`, and `headers`. SSE is explicitly deprecated.
- Claude environment expansion leaves an unresolved placeholder with a warning if the variable is absent. aru therefore writes only the placeholder and never reads the process environment.

### Fail-closed boundary

An adapter first renders a pure semantic entry. If any selected agent cannot represent the transport or secret environment reference without embedding a value or invoking a shell, resolution fails before transaction staging. Registry commands and arguments remain arrays; aru never invokes MCP package commands itself.

## Implementation consequences

- Capability schema version is locked as `1`; changing this matrix invalidates `projection-input-hash` without invalidating package requirements.
- Existing package versions remain preferred. Adding an agent only rebuilds per-agent target selections.
- Registry transport selection is deterministic and independent of response ordering.
- Tests use fixtures and temporary Git repositories; ordinary tests do not require the live registry.
