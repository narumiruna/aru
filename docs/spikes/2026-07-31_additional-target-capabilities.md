# Additional Target Skill and MCP Capability Evidence

## Scope

This note records the official project-local contracts used to extend aru's existing targets beyond Codex and Claude. Evidence was checked on 2026-07-31. aru continues to reject any representation that would require a secret value, shell expansion, global configuration write, or lossy target conversion.

## GitHub Copilot

- [GitHub's Agent Skills documentation](https://docs.github.com/en/copilot/concepts/agents/about-agent-skills) accepts project skills under `.github/skills`, `.claude/skills`, or `.agents/skills`. aru uses the target-native `.github/skills/<name>` destination.
- [Copilot CLI's MCP documentation](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers) documents committed project configuration at `.github/mcp.json`, with an `mcpServers` object and `stdio` / `http` transports. It explicitly says Copilot CLI does not read VS Code's `.vscode/mcp.json` because that file uses the incompatible `servers` top-level key.
- aru therefore scopes this adapter to Copilot CLI, renders `tools = ["*"]` to preserve the complete declared server capability, and stores only `${ENV}` references. It does not write GitHub.com repository MCP settings or `.vscode/mcp.json`.

## pi

- The installed pi documentation (`docs/skills.md`) and [upstream pi Skills documentation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md) list `.pi/skills/` and `.agents/skills/` as project locations. aru uses `.pi/skills/<name>` as pi's native destination.
- pi's documented philosophy intentionally has no built-in MCP; MCP requires a user-selected extension. aru does not install or configure executable extensions implicitly, so pi remains MCP-incompatible and fails before writes.

## OpenCode

- [OpenCode Agent Skills](https://opencode.ai/docs/skills/) lists `.opencode/skills/<name>/SKILL.md` as its project-native location, alongside compatibility locations. aru uses the native path.
- [OpenCode MCP servers](https://opencode.ai/docs/mcp-servers/) defines local servers as `type = "local"` with a command array and remote servers as `type = "remote"` with a URL and optional headers in the project `opencode.json` configuration.
- [OpenCode config](https://opencode.ai/docs/config/) documents JSON and JSONC, project-level `opencode.json`, and `{env:VARIABLE_NAME}` substitution. aru uses lossless JSONC edits so unrelated entries, comments, trailing commas, and formatting survive.
- For environment-backed HTTP headers, aru renders `{env:ENV}` placeholders. When such headers are present it renders `oauth = false`, matching OpenCode's API-key guidance and avoiding an unintended automatic OAuth flow.

## Fail-closed decisions

- Existing Codex and Claude paths and formats do not change.
- Skills use one independently owned native destination per selected target. On Unix, non-Codex paths may link to the selected Codex `.agents` copy; other platforms receive verified copies.
- Copilot, Claude, Codex, and OpenCode support aru's stdio and streamable-HTTP MCP abstractions. pi does not.
- Existing target config roots and MCP containers must have the documented object shape. Malformed, ambiguous, unmanaged same-name, or drifted managed entries block the complete transaction.
- The adapter capability schema changes whenever these projection semantics change, invalidating stale projection hashes without changing package resolution identity.
