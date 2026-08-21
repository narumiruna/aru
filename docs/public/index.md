---
hide:
  - navigation
  - toc
---

<div class="aru-hero" markdown>
<div class="aru-hero__copy" markdown>

<span class="aru-eyebrow">Coding-agent project manager</span>

# One manifest. Every agent in sync.

Aru keeps instructions, Agent Skills, and MCP servers consistent across generic Agents, Codex, Claude Code, GitHub Copilot CLI, pi, OpenCode, and a registry of project-scoped skill-only targets—without taking ownership of content it cannot safely manage.

[Get started](getting-started.md){ .md-button .md-button--primary }
[See how sync works](sync.md){ .md-button }

</div>
<div class="aru-terminal" role="img" aria-label="Terminal showing aru synchronization output">
<div class="aru-terminal__bar"><span></span><span></span><span></span></div>
<pre><code><span class="aru-prompt">$</span> aru sync --locked
<span class="aru-status">      Locked</span> skill review 1.2.0
<span class="aru-status">     Created</span> skill review (.agents/skills/review)
<span class="aru-status">    Finished</span> Project synchronized.</code></pre>
</div>
</div>

## Declare intent, pin the result

Aru separates the configuration your team maintains from the exact state every machine replays:

1. **Declare** project targets and dependencies in `aru.toml`.
2. **Lock** exact Git revisions, metadata, and projections in `aru.lock`.
3. **Sync** each agent's native project files with ownership and drift checks.

<div class="grid cards" markdown>

-   :material-file-document-check-outline:{ .lg .middle } **Instructions**

    ---

    Keep `AGENTS.md` canonical while projecting compatible imports and path rules for other agents.

    [Manage instructions](instructions.md)

-   :material-puzzle-outline:{ .lg .middle } **Agent Skills**

    ---

    Select skills from Git repositories, pin their revisions, and deploy each target's native layout.

    [Manage skills](skills.md)

-   :material-server-network:{ .lg .middle } **MCP servers**

    ---

    Configure Registry packages, HTTPS endpoints, or argv-safe stdio commands without storing secrets.

    [Manage MCP](mcp.md)

-   :material-package-variant-closed:{ .lg .middle } **Native packages**

    ---

    Compose reusable instructions, skills, trusted MCP declarations, and bounded package dependencies.

    [Manage packages](packages.md)

-   :material-connection:{ .lg .middle } **Plugin dependencies**

    ---

    Resolve selected portable skills and safe MCP from Agent Plugins, OpenAI plugins, and Gemini extensions.

    [Manage plugins](plugins.md)

</div>

## Built to fail closed

Aru validates the complete operation before writing. It preserves drifted or unowned content for review, never executes configured MCP commands, and routes multi-file updates through durable transactions with digest-gated recovery.

[Read the safety model](safety.md){ .md-button }
