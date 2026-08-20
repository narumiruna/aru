# MCP servers

Aru supports Registry packages, direct HTTPS endpoints, and direct stdio commands. It stores environment-variable names or target-native placeholders—never secret values—and never executes configured direct commands.

## Registry package

```console
aru mcp add io.example/context \
  --name context \
  --transport stdio \
  --package-registry npm
```

Registry packages support npm records rendered through exact `npx` argv and PyPI records that explicitly declare the `uvx` runtime hint:

```console
aru mcp add io.example/python-context \
  --name context \
  --transport stdio \
  --package-registry pypi
```

A candidate must be unique after transport, package registry, and target capability filtering. Aru does not silently choose the first result when metadata is ambiguous.

## HTTPS endpoint

```console
aru mcp add \
  --url https://docs.example.com/mcp \
  --name docs \
  --bearer-token-env DOCS_MCP_TOKEN \
  --header-env X-Workspace=DOCS_MCP_WORKSPACE
```

Repeat `--header-env HEADER=ENV` for non-Authorization headers backed by environment variables. Header names are case-insensitive and cannot collide. Use `--bearer-token-env` for Authorization.

Narrow a declaration to selected MCP-capable targets:

```console
aru mcp add \
  --url https://docs.example.com/mcp \
  --name docs \
  --target claude
```

## Direct stdio command

```console
aru mcp add \
  --command uvx \
  --arg=--with \
  --arg 'mcp<2' \
  --arg yfmcp@0.12.2 \
  --env-var YFINANCE_API_KEY \
  --name yfinance
```

Commands and repeated `--arg` values remain an ordered argv array. Use `--arg=--flag` when an argument starts with `-`, and pin package versions explicitly.

!!! note
    Aru validates and projects direct commands but does not execute them during add, lock, or sync.

## Update and remove

```console
aru mcp list
aru mcp update --dry-run
aru mcp update context
aru mcp remove docs
```

`mcp update` unlocks only selected Registry packages. Direct URLs and stdio commands have no Registry version to upgrade.

## Target support

Project MCP is supported for Codex, Claude Code, GitHub Copilot CLI, and OpenCode.
Agents and pi have no built-in MCP and are rejected as MCP dependency targets.

Copilot uses `.github/mcp.json`; aru does not emit VS Code's incompatible `.vscode/mcp.json` or modify GitHub.com repository settings.
