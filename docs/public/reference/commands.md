# Command reference

Run `aru <command> --help` for the complete option contract installed with your version.

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
| `aru skill add <source>` | Add selected Agent Skills |
| `aru skill list` | List locked skill sources |
| `aru skill update [source]` | Update all or selected skills |
| `aru skill remove <source>` | Remove selected skills or a source |
| `aru mcp add ...` | Add a Registry, HTTPS, or stdio MCP server |
| `aru mcp list` | List configured MCP servers |
| `aru mcp update [name]` | Update Registry-backed servers |
| `aru mcp remove <name>` | Remove a configured server |
| `aru target add <target>` | Add targets to the project set |
| `aru target remove <target>` | Remove targets from the project set |
| `aru target set <target>` | Replace the complete target set |
| `aru target list` | List configured targets |

## Inspection and export

| Command | Purpose |
| --- | --- |
| `aru tree` | Display the locked native-package dependency graph |
| `aru info <package>` | Inspect one locked or available native package |
| `aru metadata --format-version 1` | Emit versioned machine-readable metadata |
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
| `--color auto\|always\|never` | Control status color |
| `--no-progress` | Hide progress output |
