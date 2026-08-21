# Skill Target Canonical Mapping

## Decision

This note fixes the canonical target identifiers and CLI aliases used by the first project-scoped skill target registry expansion.
It classifies every Vercel `skills` CLI entry pinned in `docs/spikes/2026-08-21_vercel-skills-placement-paths.md` at `v1.5.23`, commit `435076e`.
The user-approved interoperability matrix is accepted as placement evidence for skills only.
Existing aru targets retain their previously verified instruction, skill, MCP, and native-path contracts.
No new target receives instruction or MCP capability from this matrix.

Canonical identifiers are lowercase kebab-case and are the only names serialized to `aru.toml`, `aru.lock`, metadata, and exports.
An alias is accepted only at CLI input boundaries and normalizes immediately to its canonical identifier.
Paths are explicit project-relative registry values rather than values derived at runtime.

## Classification

`existing` preserves an aru target and its established capabilities.
`new` adds a skill-only target whose conventional name and project directory agree.
`alias` maps an upstream name to a canonical target.
`exception` adds a skill-only target whose generic, nested, vendor-prefixed, or differently named directory requires an explicit mapping.
No pinned row is deferred in this approved expansion.

| Upstream CLI name | Classification | Canonical target | Project skill directory | Capabilities | Alias owner / shared group |
| --- | --- | --- | --- | --- | --- |
| `aider-desk` | new | `aider-desk` | `.aider-desk/skills` | skills | — |
| `amp` | new | `amp` | `.agents/skills` | skills | `.agents` shared group |
| `antigravity` | new | `antigravity` | `.agents/skills` | skills | `.agents` shared group |
| `antigravity-cli` | alias | `antigravity` | `.agents/skills` | skills | alias of `antigravity`; `.agents` shared group |
| `astrbot` | exception | `astrbot` | `data/skills` | skills | generic-root exception |
| `autohand-code` | alias | `autohand` | `.autohand/skills` | skills | alias of `autohand` |
| `augment` | new | `augment` | `.augment/skills` | skills | — |
| `bob` | new | `bob` | `.bob/skills` | skills | — |
| `claude-code` | alias | `claude` | `.claude/skills` | instructions, skills, MCP | alias of existing `claude` |
| `openclaw` | exception | `openclaw` | `skills` | skills | generic-root exception |
| `cline` | new | `cline` | `.agents/skills` | skills | `.agents` shared group |
| `codearts-agent` | alias | `codearts` | `.codeartsdoer/skills` | skills | alias plus vendor-directory exception |
| `codebuddy` | new | `codebuddy` | `.codebuddy/skills` | skills | — |
| `codemaker` | new | `codemaker` | `.codemaker/skills` | skills | — |
| `codestudio` | new | `codestudio` | `.codestudio/skills` | skills | — |
| `codex` | existing | `codex` | `.agents/skills` | instructions, skills, MCP | `.agents` shared group |
| `command-code` | alias | `commandcode` | `.commandcode/skills` | skills | alias of `commandcode` |
| `continue` | new | `continue` | `.continue/skills` | skills | — |
| `cortex` | new | `cortex` | `.cortex/skills` | skills | — |
| `crush` | new | `crush` | `.crush/skills` | skills | — |
| `cursor` | new | `cursor` | `.agents/skills` | skills | `.agents` shared group |
| `deepagents` | new | `deepagents` | `.agents/skills` | skills | `.agents` shared group |
| `devin` | new | `devin` | `.devin/skills` | skills | — |
| `dexto` | new | `dexto` | `.agents/skills` | skills | `.agents` shared group |
| `droid` | exception | `droid` | `.factory/skills` | skills | vendor-directory exception |
| `eve` | exception | `eve` | `agent/skills` | skills | generic-root exception |
| `firebender` | new | `firebender` | `.agents/skills` | skills | `.agents` shared group |
| `forgecode` | alias | `forge` | `.forge/skills` | skills | alias of `forge` |
| `gemini-cli` | alias | `gemini` | `.agents/skills` | skills | alias of `gemini`; `.agents` shared group |
| `github-copilot` | alias | `copilot` | `.github/skills` | instructions, skills, MCP | alias of existing `copilot`; aru native-path override |
| `goose` | new | `goose` | `.goose/skills` | skills | — |
| `grok` | new | `grok` | `.grok/skills` | skills | — |
| `hermes-agent` | alias | `hermes` | `.hermes/skills` | skills | alias of `hermes` |
| `inference-sh` | exception | `inference-sh` | `.inferencesh/skills` | skills | directory-spelling exception |
| `jazz` | new | `jazz` | `.jazz/skills` | skills | — |
| `junie` | new | `junie` | `.junie/skills` | skills | — |
| `iflow-cli` | alias | `iflow` | `.iflow/skills` | skills | alias of `iflow` |
| `kilo` | exception | `kilo` | `.kilocode/skills` | skills | directory-spelling exception |
| `kimchi` | new | `kimchi` | `.kimchi/skills` | skills | — |
| `kimi-code-cli` | alias | `kimi` | `.agents/skills` | skills | alias of `kimi`; `.agents` shared group |
| `kiro-cli` | alias | `kiro` | `.kiro/skills` | skills | alias of `kiro` |
| `kode` | new | `kode` | `.kode/skills` | skills | — |
| `lingma` | new | `lingma` | `.lingma/skills` | skills | — |
| `loaf` | new | `loaf` | `.agents/skills` | skills | `.agents` shared group |
| `mcpjam` | new | `mcpjam` | `.mcpjam/skills` | skills | — |
| `minimax-code` | alias | `minimax` | `.minimax/skills` | skills | alias of `minimax` |
| `mistral-vibe` | alias | `vibe` | `.vibe/skills` | skills | alias of `vibe` |
| `moxby` | new | `moxby` | `.moxby/skills` | skills | — |
| `mux` | new | `mux` | `.mux/skills` | skills | — |
| `opencode` | existing | `opencode` | `.opencode/skills` | instructions, skills, MCP | aru native-path override |
| `openhands` | new | `openhands` | `.openhands/skills` | skills | — |
| `ona` | new | `ona` | `.ona/skills` | skills | — |
| `pi` | existing | `pi` | `.pi/skills` | instructions, skills | — |
| `posit-assistant` | alias | `posit` | `.posit/assistant/skills` | skills | alias plus nested-path exception |
| `qoder` | new | `qoder` | `.qoder/skills` | skills | `.qoder` shared group |
| `qoder-cn` | alias | `qoder` | `.qoder/skills` | skills | alias of `qoder`; `.qoder` shared group |
| `qwen-code` | alias | `qwen` | `.qwen/skills` | skills | alias of `qwen` |
| `replit` | new | `replit` | `.agents/skills` | skills | `.agents` shared group |
| `reasonix` | new | `reasonix` | `.reasonix/skills` | skills | — |
| `rovodev` | new | `rovodev` | `.rovodev/skills` | skills | — |
| `roo` | new | `roo` | `.roo/skills` | skills | — |
| `tabnine-cli` | alias | `tabnine` | `.tabnine/agent/skills` | skills | alias plus nested-path exception |
| `terramind` | new | `terramind` | `.terramind/skills` | skills | — |
| `tinycloud` | new | `tinycloud` | `.tinycloud/skills` | skills | — |
| `trae` | new | `trae` | `.trae/skills` | skills | `.trae` shared group |
| `trae-cn` | alias | `trae` | `.trae/skills` | skills | alias of `trae`; `.trae` shared group |
| `warp` | new | `warp` | `.agents/skills` | skills | `.agents` shared group |
| `windsurf` | new | `windsurf` | `.windsurf/skills` | skills | — |
| `zed` | new | `zed` | `.agents/skills` | skills | `.agents` shared group |
| `zcode` | new | `zcode` | `.zcode/skills` | skills | — |
| `zencoder` | new | `zencoder` | `.zencoder/skills` | skills | `.zencoder` shared group |
| `zenflow` | alias | `zencoder` | `.zencoder/skills` | skills | alias of `zencoder`; `.zencoder` shared group |
| `neovate` | new | `neovate` | `.neovate/skills` | skills | — |
| `pochi` | new | `pochi` | `.pochi/skills` | skills | — |
| `promptscript` | new | `promptscript` | `.agents/skills` | skills | `.agents` shared group |
| `adal` | new | `adal` | `.adal/skills` | skills | — |
| `universal` | alias | `agents` | `.agents/skills` | instructions, skills | alias of existing `agents`; `.agents` shared group |

## Registry invariants

- The table contains 77 source rows and every pinned Vercel CLI name appears exactly once.
- The implementation contains 73 canonical targets because Antigravity, Claude, Copilot, Autohand, CodeArts, Command Code, ForgeCode, Gemini, Hermes, iFlow, Kimi, Kiro, MiniMax, Vibe, Posit, Qoder CN, Qwen Code, Tabnine CLI, Trae CN, Zenflow, and Universal normalize through aliases or existing identifiers.
- Alias strings and canonical identifiers share one collision-free CLI input namespace.
- Every project directory is normalized, project-relative, and outside `.aru/`.
- Existing aru destinations remain `.agents/skills`, `.claude/skills`, `.github/skills`, `.pi/skills`, and `.opencode/skills` as applicable.
- Copilot and OpenCode intentionally retain aru's independently verified native project paths instead of the Vercel universal-path entries.
- All new canonical targets are skill-only.
- Global paths are naming evidence only and are not installation destinations in this expansion.

## Evidence

- `docs/spikes/2026-08-21_vercel-skills-placement-paths.md` records the pinned Vercel registry and installation behavior.
- `third_party/reference/vercel-skills/src/agents.ts` supplies the 77 upstream names and project paths.
- `docs/spikes/2026-07-31_additional-target-capabilities.md` records the official-path evidence retained for existing aru targets.
- Registry unit tests enforce row count, canonical syntax, alias uniqueness, path safety, capability boundaries, and every source-name mapping.
