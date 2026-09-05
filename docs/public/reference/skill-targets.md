# Skill targets

In initialized projects, aru keeps target identifiers short and stores only the canonical names shown below.
CLI aliases are accepted by `init`, `target add`, `target remove`, `target set`, and dependency target options, then normalized before `aru.toml` is written.
Run `aru target list --available` to print this registry from the installed binary.

Targets marked `skills` only receive Agent Skills.
They do not receive instructions or MCP servers unless a later adapter adds independently verified support.
Implicit instruction and MCP reach is restricted to capable configured targets, and an explicit unsupported target is rejected before writes.

The project skill directories below are relative to the project root and end with `<skill-name>` when projected.
Standalone `aru skill add --global` instead installs into target-native user directories, including supported environment overrides and distinct global aliases.
See [standalone skill installation](../skills.md#standalone-installation) for global paths, restrictions, and examples.

| Canonical target | Project skill directory | Capabilities | CLI aliases |
| --- | --- | --- | --- |
| `adal` | `.adal/skills` | skills | — |
| `agents` | `.agents/skills` | instructions, skills | `universal` |
| `aider-desk` | `.aider-desk/skills` | skills | — |
| `amp` | `.agents/skills` | skills | — |
| `antigravity` | `.agents/skills` | skills | `antigravity-cli` |
| `astrbot` | `data/skills` | skills | — |
| `augment` | `.augment/skills` | skills | — |
| `autohand` | `.autohand/skills` | skills | `autohand-code` |
| `bob` | `.bob/skills` | skills | — |
| `claude` | `.claude/skills` | instructions, skills, MCP | `claude-code` |
| `cline` | `.agents/skills` | skills | — |
| `codearts` | `.codeartsdoer/skills` | skills | `codearts-agent` |
| `codebuddy` | `.codebuddy/skills` | skills | — |
| `codemaker` | `.codemaker/skills` | skills | — |
| `codestudio` | `.codestudio/skills` | skills | — |
| `codex` | `.agents/skills` | instructions, skills, MCP | — |
| `commandcode` | `.commandcode/skills` | skills | `command-code` |
| `continue` | `.continue/skills` | skills | — |
| `copilot` | `.github/skills` | instructions, skills, MCP | `github-copilot` |
| `cortex` | `.cortex/skills` | skills | — |
| `crush` | `.crush/skills` | skills | — |
| `cursor` | `.agents/skills` | skills | — |
| `deepagents` | `.agents/skills` | skills | — |
| `devin` | `.devin/skills` | skills | — |
| `dexto` | `.agents/skills` | skills | — |
| `droid` | `.factory/skills` | skills | — |
| `eve` | `agent/skills` | skills | — |
| `firebender` | `.agents/skills` | skills | — |
| `forge` | `.forge/skills` | skills | `forgecode` |
| `gemini` | `.agents/skills` | skills | `gemini-cli` |
| `goose` | `.goose/skills` | skills | — |
| `grok` | `.grok/skills` | skills | — |
| `hermes` | `.hermes/skills` | skills | `hermes-agent` |
| `iflow` | `.iflow/skills` | skills | `iflow-cli` |
| `inference-sh` | `.inferencesh/skills` | skills | — |
| `jazz` | `.jazz/skills` | skills | — |
| `junie` | `.junie/skills` | skills | — |
| `kilo` | `.kilocode/skills` | skills | — |
| `kimchi` | `.kimchi/skills` | skills | — |
| `kimi` | `.agents/skills` | skills | `kimi-code-cli` |
| `kiro` | `.kiro/skills` | skills | `kiro-cli` |
| `kode` | `.kode/skills` | skills | — |
| `lingma` | `.lingma/skills` | skills | — |
| `loaf` | `.agents/skills` | skills | — |
| `mcpjam` | `.mcpjam/skills` | skills | — |
| `minimax` | `.minimax/skills` | skills | `minimax-code` |
| `moxby` | `.moxby/skills` | skills | — |
| `mux` | `.mux/skills` | skills | — |
| `neovate` | `.neovate/skills` | skills | — |
| `ona` | `.ona/skills` | skills | — |
| `openclaw` | `skills` | skills | — |
| `opencode` | `.opencode/skills` | instructions, skills, MCP | — |
| `openhands` | `.openhands/skills` | skills | — |
| `pi` | `.pi/skills` | instructions, skills | — |
| `pochi` | `.pochi/skills` | skills | — |
| `posit` | `.posit/assistant/skills` | skills | `posit-assistant` |
| `promptscript` | `.agents/skills` | skills | — |
| `qoder` | `.qoder/skills` | skills | `qoder-cn` |
| `qwen` | `.qwen/skills` | skills | `qwen-code` |
| `reasonix` | `.reasonix/skills` | skills | — |
| `replit` | `.agents/skills` | skills | — |
| `roo` | `.roo/skills` | skills | — |
| `rovodev` | `.rovodev/skills` | skills | — |
| `tabnine` | `.tabnine/agent/skills` | skills | `tabnine-cli` |
| `terramind` | `.terramind/skills` | skills | — |
| `tinycloud` | `.tinycloud/skills` | skills | — |
| `trae` | `.trae/skills` | skills | `trae-cn` |
| `vibe` | `.vibe/skills` | skills | `mistral-vibe` |
| `warp` | `.agents/skills` | skills | — |
| `windsurf` | `.windsurf/skills` | skills | — |
| `zcode` | `.zcode/skills` | skills | — |
| `zed` | `.agents/skills` | skills | — |
| `zencoder` | `.zencoder/skills` | skills | `zenflow` |

In initialized projects, when multiple selected targets share a destination, aru creates one owned projection while retaining every canonical target in the lock.
On Unix, a target-specific destination may link to a selected `.agents/skills` projection.
The relative link is calculated from the actual destination depth, including nested paths such as `.posit/assistant/skills` and `.tabnine/agent/skills`.
Platforms without project symlink support receive verified copies.
