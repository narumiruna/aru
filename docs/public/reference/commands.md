# Command reference

Run `aru <command> --help` for the complete option contract installed with your version.

## Interactive behavior

Prompts require both stdin and stderr to be terminals and are disabled by `--no-interactive`.
Explicit arguments bypass the corresponding menu, not necessarily every menu in a command.

| Command | Omitted selection in a terminal | Without prompts |
| --- | --- | --- |
| `init` | Choose project targets | Requires `--target` |
| `target add/remove/set` | Choose unconfigured, configured, or replacement targets respectively | Requires targets |
| `skill add SOURCE` | Choose Project/Global scope, then targets and skills | Defaults to Project; requires explicit skills and, for standalone installs, targets |
| `add SOURCE`, `plugin add SOURCE`, `mcp add ...` | Choose configured dependency targets; standalone MCP offers MCP-capable targets | Uses configured targets; standalone MCP requires `--target` |
| `instruction add` | Enter one exact project-relative `AGENTS.md` path; no directory scan | Requires file paths |
| `instruction remove` | Choose declared file selectors, including declared globs | Requires selectors |
| `remove`, `skill remove`, `plugin remove`, `mcp remove` | Choose one declared source or name | Requires a source or name |
| `update`, `skill update`, `plugin update`, `mcp update` | Choose items with all eligible items initially checked | Updates all, as before |

Native package updates include locked transitive packages. The MCP update menu lists only Registry-backed servers.
Managed add menus offer configured targets, initially checked; skill and MCP menus filter by capability.
`target set` initially checks the current target set. Other mutation menus start unchecked.

Use arrows to move, space to toggle multi-select choices, typing to filter, Enter to accept, and Esc to cancel.
Multi-select menus require at least one item. Removing the final project target still fails; use `target set` to replace it.
An empty inventory produces an actionable error instead of an empty menu.
Canceling does not apply project or target changes. Menu preparation may create private per-user coordination metadata outside the project.
Managed selections are checked against `aru.toml` and `aru.lock` again under the operation lock before execution; concurrent changes require a retry.
`--dry-run` still prompts for missing selections but only previews the result.

Only skill installation supports Global scope. `--scope project` and `--scope global` select it explicitly; `--global` is a shorthand for Global and conflicts with `--scope`.
`--project PATH` remains a directory override, not a scope flag.
Source identifiers, MCP source/name/options, plugin component selection, and `--trust-mcp` remain explicit. Menus never imply trust or `--force`.
Listing, inspection, export, metadata, `sync`, `lock`, packaging, shell completion, and self-update do not ask for input.

### Inline terminal output

When stdin and stderr are terminals and `--no-interactive` is absent:

- Menus share colors and keyboard hints. Target labels include destinations and supported project capabilities; this does not add Global support for other resources.
- Step hints explain the current choice and what follows. Explicit selections still skip the corresponding menus.
- Initialization, managed reconciliation (including `sync` and `lock`), and standalone installation show an informational plan before applying changes. Summaries show at most 12 actions and shorten long lines. Commands supporting `--dry-run` retain a full preview without applying; `init` does not support `--dry-run`.
- Resolution and application use transient spinners, cleared before menus, warnings, results, or errors. `TERM=dumb` uses static progress instead.

The plan is not an approval prompt and does not bypass validation, ownership, trust, or transaction checks. No additional Enter is required. Dry runs keep their full previews and never enter the application phase.

`--no-progress` hides spinners and progress status, but keeps menus, step hints, and summaries. `--quiet` also hides hints, summaries, and routine completion output, but keeps required prompts and warnings. `--color never` removes color, not cursor movement in menus or spinners.

`--no-interactive` disables menus, step hints, summaries, and animation; existing static status remains. If stdin or stderr is not a terminal, aru also omits these interactive elements. Static progress may still appear when stderr is a terminal. Machine-readable and list output stays on stdout; human interaction and status stay on stderr.

```console
aru init
aru skill add owner/repository
aru target set
aru skill update --dry-run
aru skill add owner/repository --scope project --target codex --all
aru update --no-interactive
```

## Project lifecycle

| Command | Purpose |
| --- | --- |
| `aru init --target <target>` | Initialize an aru project |
| `aru lock` | Update `aru.lock` without projecting files |
| `aru sync` | Reconcile the lock and configured target paths |
| `aru audit` | Inspect project integrity without changing state |

## Native packages

| Command | Purpose |
| --- | --- |
| `aru add <source>` | Add a native package dependency |
| `aru remove <source>` | Remove a direct native package dependency |
| `aru update [package]` | Update all or selected native packages |
| `aru package` | Build a verified deterministic package archive |

## Resource management

| Command | Purpose |
| --- | --- |
| `aru instruction add <file>...` | Add exact project-relative `AGENTS.md` sources |
| `aru instruction list` | List declared instruction selectors |
| `aru instruction remove <file>` | Remove exact instruction selectors |
| `aru skill add <source>` | Add managed skills or install them once without initialization |
| `aru skill list` | List locked skill sources |
| `aru skill update [source]` | Update all or selected skills |
| `aru skill remove <source>` | Remove selected skills or a source |
| `aru mcp add ...` | Add a managed MCP server or install one once without initialization |
| `aru mcp list` | List configured MCP servers |
| `aru mcp update [name]` | Update Registry-backed servers |
| `aru mcp remove <name>` | Remove a configured server |
| `aru plugin info <source>` | Inspect a locked or available plugin |
| `aru plugin add <source>` | Add selected plugin resources |
| `aru plugin list` | List configured plugins |
| `aru plugin update [name]` | Update all or selected plugins |
| `aru plugin remove <name>` | Remove a complete plugin declaration |
| `aru target add <target>` | Add targets to the project set |
| `aru target remove <target>` | Remove targets from the project set |
| `aru target set <target>` | Replace the complete target set |
| `aru target list` | List configured canonical targets |
| `aru target list --available` | List canonical targets, skill paths, capabilities, and aliases |

## Inspection and export

| Command | Purpose |
| --- | --- |
| `aru tree` | Display the locked native-package dependency graph |
| `aru info <package>` | Inspect one locked or available native package |
| `aru metadata --format-version 1` | Emit shape-compatible machine-readable metadata |
| `aru metadata --format-version 2` | Emit metadata with plugins and resource origins |
| `aru export --format cyclonedx1.5` | Export deterministic CycloneDX inventory |

## Utilities

| Command | Purpose |
| --- | --- |
| `aru generate-shell-completion <shell>` | Generate Bash, Zsh, or Fish completion |
| `aru self update` | Update a standalone aru installation |

## Global options

| Option | Meaning |
| --- | --- |
| `--project <path>` | Discover the project from another directory |
| `--locked` | Fail if the command would change `aru.lock` |
| `--offline` | Disable remote Git and Registry access |
| `--frozen` | Equivalent to `--locked --offline` |
| `-q`, `--quiet` | Suppress routine status output |
| `-v`, `--verbose` | Show more detail; repeat for projection identity detail |
| `--color auto\|always\|never` | Control menu and status color |
| `--no-progress` | Hide spinners and progress status |
| `--no-interactive` | Disable prompts, step hints, summaries, and animation; preserve defaults and reject missing required selections |
