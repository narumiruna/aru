# Vercel Skills Placement Path Reference

## Purpose and snapshot

This note records where the Vercel `skills` CLI recognizes and places agent skills.
It is implementation research rather than aru's public target contract.
The evidence comes from `third_party/reference/vercel-skills` at tag `v1.5.23`, commit `435076e`.
The reference can change, so re-check the pinned source before using this matrix for product behavior.

A skill is a directory whose required entry point is `SKILL.md`.
The resulting layout is `<skills-directory>/<skill-name>/SKILL.md`, with optional sibling resources such as `scripts/`, `references/`, and `assets/`.

## Canonical installation model

The default `symlink` mode stores skill content in one canonical directory.

| Scope | Canonical content directory |
| --- | --- |
| Project | `<project>/.agents/skills/<skill-name>/` |
| Global | `~/.agents/skills/<skill-name>/` |

Agents whose project path is `.agents/skills/` are called universal agents by the reference implementation.
Universal agents use the canonical directory directly.
For a non-universal agent, the default mode copies content into the canonical directory and links the agent-specific path to it.
The `copy` mode instead copies content directly to the agent-specific path.

For project symlink installs, a non-universal agent-specific link is normally skipped when that agent's project root does not already exist.
The skill remains available in the canonical `.agents/skills/` directory.
Claude Code is the explicit exception and may receive a new `.claude/skills/` link even when `.claude/` does not exist yet.

For universal global installs, the installer keeps the content in `~/.agents/skills/` and does not create a second link at the registry's agent-specific global path.
This makes some registered global paths documentation or detection metadata rather than the physical destination used by the default universal install flow.

## Project and registered global paths

`Project path` is relative to the project root.
`Registered global path` is the value declared by the agent registry, before the universal-agent canonical override described above.

| Agent | CLI name | Project path | Registered global path |
| --- | --- | --- | --- |
| AiderDesk | `aider-desk` | `.aider-desk/skills/` | `~/.aider-desk/skills/` |
| Amp | `amp` | `.agents/skills/` | `~/.config/agents/skills/` |
| Antigravity | `antigravity` | `.agents/skills/` | `~/.gemini/antigravity/skills/` |
| Antigravity CLI | `antigravity-cli` | `.agents/skills/` | `~/.gemini/antigravity-cli/skills/` |
| AstrBot | `astrbot` | `data/skills/` | `~/.astrbot/data/skills/` |
| Autohand Code CLI | `autohand-code` | `.autohand/skills/` | `~/.autohand/skills/` |
| Augment | `augment` | `.augment/skills/` | `~/.augment/skills/` |
| IBM Bob | `bob` | `.bob/skills/` | `~/.bob/skills/` |
| Claude Code | `claude-code` | `.claude/skills/` | `~/.claude/skills/` |
| OpenClaw | `openclaw` | `skills/` | `~/.openclaw/skills/` |
| Cline | `cline` | `.agents/skills/` | `~/.agents/skills/` |
| CodeArts Agent | `codearts-agent` | `.codeartsdoer/skills/` | `~/.codeartsdoer/skills/` |
| CodeBuddy | `codebuddy` | `.codebuddy/skills/` | `~/.codebuddy/skills/` |
| Codemaker | `codemaker` | `.codemaker/skills/` | `~/.codemaker/skills/` |
| Code Studio | `codestudio` | `.codestudio/skills/` | `~/.codestudio/skills/` |
| Codex | `codex` | `.agents/skills/` | `~/.codex/skills/` |
| Command Code | `command-code` | `.commandcode/skills/` | `~/.commandcode/skills/` |
| Continue | `continue` | `.continue/skills/` | `~/.continue/skills/` |
| Cortex Code | `cortex` | `.cortex/skills/` | `~/.snowflake/cortex/skills/` |
| Crush | `crush` | `.crush/skills/` | `~/.config/crush/skills/` |
| Cursor | `cursor` | `.agents/skills/` | `~/.cursor/skills/` |
| Deep Agents | `deepagents` | `.agents/skills/` | `~/.deepagents/agent/skills/` |
| Devin for Terminal | `devin` | `.devin/skills/` | `~/.config/devin/skills/` |
| Dexto | `dexto` | `.agents/skills/` | `~/.agents/skills/` |
| Droid | `droid` | `.factory/skills/` | `~/.factory/skills/` |
| Eve | `eve` | `agent/skills/` | Not supported |
| Firebender | `firebender` | `.agents/skills/` | `~/.firebender/skills/` |
| ForgeCode | `forgecode` | `.forge/skills/` | `~/.forge/skills/` |
| Gemini CLI | `gemini-cli` | `.agents/skills/` | `~/.gemini/skills/` |
| GitHub Copilot | `github-copilot` | `.agents/skills/` | `~/.copilot/skills/` |
| Goose | `goose` | `.goose/skills/` | `~/.config/goose/skills/` |
| Grok Build | `grok` | `.grok/skills/` | `~/.grok/skills/` |
| Hermes Agent | `hermes-agent` | `.hermes/skills/` | `~/.hermes/skills/` |
| inference.sh | `inference-sh` | `.inferencesh/skills/` | `~/.inferencesh/skills/` |
| Jazz | `jazz` | `.jazz/skills/` | `~/.jazz/skills/` |
| Junie | `junie` | `.junie/skills/` | `~/.junie/skills/` |
| iFlow CLI | `iflow-cli` | `.iflow/skills/` | `~/.iflow/skills/` |
| Kilo Code | `kilo` | `.kilocode/skills/` | `~/.kilocode/skills/` |
| Kimchi | `kimchi` | `.kimchi/skills/` | `~/.config/kimchi/harness/skills/` |
| Kimi Code CLI | `kimi-code-cli` | `.agents/skills/` | `~/.agents/skills/` |
| Kiro CLI | `kiro-cli` | `.kiro/skills/` | `~/.kiro/skills/` |
| Kode | `kode` | `.kode/skills/` | `~/.kode/skills/` |
| Lingma | `lingma` | `.lingma/skills/` | `~/.lingma/skills/` |
| Loaf | `loaf` | `.agents/skills/` | `~/.agents/skills/` |
| MCPJam | `mcpjam` | `.mcpjam/skills/` | `~/.mcpjam/skills/` |
| MiniMax Code | `minimax-code` | `.minimax/skills/` | `~/.minimax/skills/` |
| Mistral Vibe | `mistral-vibe` | `.vibe/skills/` | `~/.vibe/skills/` |
| Moxby | `moxby` | `.moxby/skills/` | `~/.moxby/skills/` |
| Mux | `mux` | `.mux/skills/` | `~/.mux/skills/` |
| OpenCode | `opencode` | `.agents/skills/` | `~/.config/opencode/skills/` |
| OpenHands | `openhands` | `.openhands/skills/` | `~/.openhands/skills/` |
| Ona | `ona` | `.ona/skills/` | `~/.ona/skills/` |
| Pi | `pi` | `.pi/skills/` | `~/.pi/agent/skills/` |
| Posit Assistant | `posit-assistant` | `.posit/assistant/skills/` | `~/.posit/assistant/skills/` |
| Qoder | `qoder` | `.qoder/skills/` | `~/.qoder/skills/` |
| Qoder CN | `qoder-cn` | `.qoder/skills/` | `~/.qoder-cn/skills/` |
| Qwen Code | `qwen-code` | `.qwen/skills/` | `~/.qwen/skills/` |
| Replit | `replit` | `.agents/skills/` | `~/.config/agents/skills/` |
| Reasonix | `reasonix` | `.reasonix/skills/` | `~/.reasonix/skills/` |
| Rovo Dev | `rovodev` | `.rovodev/skills/` | `~/.rovodev/skills/` |
| Roo Code | `roo` | `.roo/skills/` | `~/.roo/skills/` |
| Tabnine CLI | `tabnine-cli` | `.tabnine/agent/skills/` | `~/.tabnine/agent/skills/` |
| Terramind | `terramind` | `.terramind/skills/` | `~/.terramind/skills/` |
| Tinycloud | `tinycloud` | `.tinycloud/skills/` | `~/.tinycloud/skills/` |
| Trae | `trae` | `.trae/skills/` | `~/.trae/skills/` |
| Trae CN | `trae-cn` | `.trae/skills/` | `~/.trae-cn/skills/` |
| Warp | `warp` | `.agents/skills/` | `~/.agents/skills/` |
| Windsurf | `windsurf` | `.windsurf/skills/` | `~/.codeium/windsurf/skills/` |
| Zed | `zed` | `.agents/skills/` | `~/.agents/skills/` |
| ZCode | `zcode` | `.zcode/skills/` | `~/.zcode/skills/` |
| Zencoder | `zencoder` | `.zencoder/skills/` | `~/.zencoder/skills/` |
| Zenflow | `zenflow` | `.zencoder/skills/` | `~/.zencoder/skills/` |
| Neovate | `neovate` | `.neovate/skills/` | `~/.neovate/skills/` |
| Pochi | `pochi` | `.pochi/skills/` | `~/.pochi/skills/` |
| PromptScript | `promptscript` | `.agents/skills/` | Not supported |
| AdaL | `adal` | `.adal/skills/` | `~/.adal/skills/` |
| Universal | `universal` | `.agents/skills/` | `~/.config/agents/skills/` |

## Conditional and overridden global roots

The table uses the usual Linux-style `~/.config` rendering for XDG paths.
Amp, Replit, Universal, OpenCode, Goose, and Devin derive applicable global paths from the XDG configuration root.

The following environment variables override selected global roots.

| Variable | Affected agent | Effective suffix |
| --- | --- | --- |
| `CODEX_HOME` | Codex | `skills/` |
| `CLAUDE_CONFIG_DIR` | Claude Code | `skills/` |
| `VIBE_HOME` | Mistral Vibe | `skills/` |
| `HERMES_HOME` | Hermes Agent | `skills/` |
| `AUTOHAND_HOME` | Autohand Code | `skills/` |
| `GROK_HOME` | Grok Build | `skills/` |

OpenClaw selects `~/.openclaw/skills/` when `~/.openclaw/` exists.
It falls back to `~/.clawdbot/skills/` or `~/.moltbot/skills/` when the corresponding legacy home exists.
Eve and PromptScript are project-only and reject global installation.

Eve additionally supports subagent-specific project skills at `agent/subagents/<subagent-name>/skills/<skill-name>/`.

## Discovery paths are not placement paths

Source discovery and installation use different path sets.
In particular, Codex installs project skills through `.agents/skills/`, while source discovery still recognizes `.codex/skills/`.
The same distinction applies to compatibility source paths such as `.cline/skills/`, `.github/skills/`, and `.opencode/skills/`.

The source scanner prioritizes a root `SKILL.md`, the standard `skills/` catalogs, and these compatibility containers.
Known containers are searched to a bounded depth unless full-depth discovery is requested.

```text
SKILL.md
skills/
skills/.curated/
skills/.experimental/
skills/.system/
.agents/skills/
.claude/skills/
.cline/skills/
.codebuddy/skills/
.codex/skills/
.commandcode/skills/
.continue/skills/
.github/skills/
.goose/skills/
.grok/skills/
.iflow/skills/
.junie/skills/
.kilocode/skills/
.kimchi/skills/
.kiro/skills/
.minimax/skills/
.mux/skills/
.neovate/skills/
.opencode/skills/
.openhands/skills/
.pi/skills/
.posit/assistant/skills/
.qoder/skills/
.roo/skills/
.trae/skills/
.windsurf/skills/
.zcode/skills/
.zencoder/skills/
```

Claude plugin manifests may declare additional discovery paths through `.claude-plugin/marketplace.json` or `.claude-plugin/plugin.json`.
Those manifest-declared paths are discovery inputs, not fixed installation destinations.

## Relevance to aru

Aru intentionally does not copy the complete Vercel agent registry.
Aru's supported target and projection contract remains documented in `docs/public/skills.md` and `docs/public/reference/project-files.md`.
For example, aru uses target-native `.github/skills/`, `.pi/skills/`, and `.opencode/skills/` projections, while the Vercel reference classifies the corresponding agents as universal project consumers of `.agents/skills/`.
Any future aru target expansion should verify the target's own official contract in addition to consulting this interoperability reference.

## Evidence

- `third_party/reference/vercel-skills/src/agents.ts` defines agent project paths, registered global paths, environment overrides, and universal-agent classification.
- `third_party/reference/vercel-skills/src/installer.ts` defines canonical storage, copy versus symlink behavior, universal overrides, and Eve subagent placement.
- `third_party/reference/vercel-skills/src/skills.ts` defines prioritized source-discovery containers.
- `third_party/reference/vercel-skills/src/plugin-manifest.ts` defines Claude plugin-manifest discovery.
- `third_party/reference/vercel-skills/README.md` contains the generated supported-agent and discovery summaries.
