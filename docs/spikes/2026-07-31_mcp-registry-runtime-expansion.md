# MCP Registry Runtime Expansion Evidence

Date: 2026-07-31

## Scope

This note decides which additional MCP Registry package records aru can translate into a deterministic stdio command without installing, executing, or inspecting a package. It extends the v1 decisions in [`2026-07-30_mcp-registry-target-capabilities.md`](2026-07-30_mcp-registry-target-capabilities.md).

## Evidence

- The checked-in Registry draft schema at `third_party/reference/mcp-registry/docs/reference/server-json/draft/server.schema.json` identifies `npm`, `pypi`, `cargo`, `oci`, `nuget`, and `mcpb` package registries. It describes `runtimeHint` as the client runtime hint and gives `npx`, `uvx`, `docker`, and `dnx` examples.
- The Registry's checked-in generic `server.json` reference gives a PyPI package with an exact version and `runtimeHint = "uvx"`. The same reference says Cargo has no single-shot equivalent to npm's `npx`, PyPI's `uvx`, or NuGet's `dnx`.
- A read-only request on 2026-07-31 to `GET https://registry.modelcontextprotocol.io/v0.1/servers/io.github.domdomegg%2Ftime-mcp-pypi/versions/1.0.6` returned a legacy PyPI record with `runtimeHint = "python"`; it did not identify a module or executable.
- [uv's official tool guide](https://docs.astral.sh/uv/guides/tools/) documents `uvx TOOL`, `TOOL@VERSION`, and `uvx --from PACKAGE COMMAND` when the distribution and command names differ.

## Decision matrix

| Registry type | Approved shape | Exact aru projection | Decision |
| --- | --- | --- | --- |
| npm | stdio with absent or `npx` runtime hint | Existing `npx [runtime args] --yes IDENTIFIER@VERSION [package args]` | Keep supported |
| PyPI | stdio with explicit `uvx` runtime hint and exact package version | `uvx [runtime args] IDENTIFIER@VERSION [package args]` | Add support |
| PyPI | absent, `python`, or any non-`uvx` runtime hint | None | Reject; metadata does not identify a safe single-shot command |
| Cargo | Any | None | Reject; Registry documentation says installation is separate and no single-shot runtime exists |
| OCI | Any | None | Reject; aru has no approved container invocation, environment forwarding, or digest policy |
| NuGet | Any | None | Reject; `dnx` prerequisites and exact invocation policy require separate evidence |
| MCPB | Any | None | Reject; support would require download, hash verification, extraction, and installation lifecycle ownership |
| Unknown | Any | None | Reject fail-closed |

The approved PyPI shape uses the Registry's same-name tool convention represented by `identifier` plus `runtimeHint = "uvx"`. A package whose executable differs from its distribution cannot express the required `--from PACKAGE COMMAND` distinction in the current record and remains unsupported. aru does not probe PyPI metadata or run `uvx` to guess.

## Ordering and selection

- Preserve Registry `runtimeArguments` before the versioned package selector and `packageArguments` after it.
- Reuse existing argument validation: fixed non-secret positional/named values are accepted; secret arguments and unresolved required arguments are rejected.
- Preserve unresolved secret environment inputs as names only; fixed required environment values remain unsupported.
- Lock PyPI identity as `registry = "pypi"`, the Registry identifier, and the exact package or server version.
- When npm and PyPI candidates both survive filtering, require `package-registry`; never prefer one by response order.

## Safety boundary

aru records `uvx` and argv but never invokes them during add, lock, sync, update, audit, metadata, or tests. Routine tests decode local fixtures only. Unsupported runtime hints and package types are omitted from the candidate set, producing the existing deterministic no-candidate or ambiguity diagnostics before writes.
