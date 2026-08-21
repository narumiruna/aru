# Plugin dependencies

Aru can resolve portable resources from Agent Plugins 1.0, OpenAI plugin packages, and Gemini CLI extensions.

The plugin resolver imports selected Agent Skills and a constrained MCP subset into aru's existing project targets.

It does not convert package schemas, install plugins into a host marketplace, create a Gemini target, or execute plugin code.

## Inspect a plugin

Inspect a Git source without changing project state:

```console
aru plugin info owner/repository
aru plugin info owner/monorepo --subdir plugins/review
aru plugin info ../plain-plugin-directory --format gemini
```

Format detection checks only the selected plugin root.

It does not recursively search a repository for manifests.

A canonical Agent Plugins 1.0 `plugin.json` selects the portable format.

A `.codex-plugin/plugin.json` selects OpenAI, and `gemini-extension.json` selects Gemini.

An Agent Plugins base with `extensions["com.openai"]` or a `.codex-plugin` overlay is one OpenAI composite plugin.

Other multi-format roots are ambiguous and require `--format agent-plugins`, `--format openai`, or `--format gemini`.

Agent Plugins schema versions other than the published 1.0.0 identifier are rejected.

## Add selected resources

Import all portable skills from a plugin:

```console
aru plugin add owner/review-tools --component skills
```

Import named resources and narrow their targets:

```console
aru plugin add owner/review-tools \
  --skill review \
  --mcp docs \
  --trust-mcp docs \
  --target codex \
  --target claude
```

`--component skills` conflicts with `--skill`, and `--component mcp` conflicts with `--mcp`.

Different component types may be selected together.

With no selector, aru treats the declaration as whole-plugin intent.

Whole-plugin intent fails when any active capability is unsupported or any MCP entry is unsafe.

Any explicit selector authorizes a partial import, so unselected native hooks, apps, commands, policies, themes, sub-agents, or unsafe MCP entries remain inventory rather than blockers.

A plugin name from the source manifest becomes the stable `[plugins.<name>]` key.

The detected format is persisted so locked replay never redetects a changed upstream package layout.

## Trust and safe MCP

Every selected plugin MCP server requires a name-specific trust decision.

Trust authorizes only that export and does not bypass transport, target, secret, collision, or ownership validation.

A remote plugin MCP entry is accepted only when it maps losslessly to Streamable HTTP, uses an absolute credential-free HTTPS URL, and has no headers, authentication, or variable expansion.

A stdio plugin MCP entry is accepted only when it uses one bare executable token, preserves ordered argv, and has no working directory, environment values, plugin placeholders, absolute paths, or explicit relative path arguments.

Aru cannot prove that an opaque bare command is independent of its original default working directory.

Recognized working-directory dependencies still fail closed.

SSE, bundled executables, literal headers or environment values, OAuth wiring, app identifiers, disabled servers, and unknown transports are rejected when selected.

Aru records accepted commands but never executes them during inspection, locking, synchronization, audit, or export.

## Update, list, and remove

```console
aru plugin list
aru plugin update --dry-run
aru plugin update review-tools
aru plugin update review-tools --precise 1.2.3
aru plugin remove review-tools
```

`plugin remove` removes the complete declaration and safely reconciles only digest-matching aru-owned projections.

Edit `aru.toml` and run `aru sync` to change granular selectors in this release.

Use `--no-sync` to update only `aru.toml` and `aru.lock`.

Use `--dry-run` to resolve and print the complete plan without writing project files or persistent cache.

## Locking and inspection

Plugin records require `aru.lock` version 4.

An unlocked `aru lock` or `aru sync` upgrades a valid version 3 lock deterministically.

Check, locked, and frozen modes never rewrite version 3 and report the unlocked command required for upgrade.

`aru metadata --format-version 1` keeps its existing JSON shape.

`aru metadata --format-version 2` adds plugin records and explicit skill and MCP origins.

CycloneDX export includes plugin package components and edges to selected resources.

Audit verifies the cached plugin tree and contributing manifest digests without reading secrets or executing commands.
