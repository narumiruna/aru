# Safety and recovery

Aru treats package metadata, instructions, skills, existing project files, and remote Registry responses as untrusted input.

## Fail-closed validation

Before writing, aru validates the complete requested operation. It rejects unsupported or ambiguous inputs instead of selecting a convenient fallback or silently dropping behavior.

Key boundaries include:

- bounded source discovery, file counts, file sizes, and package graph sizes;
- project-relative portable paths with no traversal or unsafe file types;
- deterministic ordering, serialization, hashing, and operation plans;
- HTTPS Registry requests with credential-free URLs, bounded redirects, timeouts, pages, records, and body sizes;
- explicit rejection of malformed metadata, duplicate exports, cycles, target incompatibility, and case-folding collisions;
- plugin format ambiguity, unsupported whole-plugin capabilities, unsafe MCP fields, and altered cached plugin content.

## Secrets and commands

MCP commands and arguments remain argv arrays.
Aru does not shell-expand package or plugin metadata and never executes configured direct MCP commands during add, lock, sync, inspection, audit, or export.

Secret-bearing configuration stores only environment-variable names or target-native placeholders. Aru does not read or persist secret values.

## Ownership protection

Aru distinguishes owned, unowned, and drifted content:

- unmanaged destinations collide by default;
- `--merge` preserves unmanaged Markdown around source-specific marker blocks;
- `--force` performs explicit destructive takeover;
- drifted owned entries are preserved and reported instead of overwritten;
- removals affect only digest-matching aru-owned output;
- unrelated TOML keys, JSON entries, and JSONC comments survive managed MCP updates.

!!! warning
    Treat `--force` as a last-resort migration action. Review and back up every colliding destination first.

## Atomic transactions

Every mutating command:

1. takes `.aru/operation.lock`;
2. rereads and validates project inputs;
3. stages each destination beside its final path;
4. writes a durable journal;
5. performs fixed-order atomic replacements with sibling backups.

A normal apply error triggers immediate rollback.

## Recover an interrupted operation

After a process kill or power loss, run a mutating command again:

```console
aru sync
```

Before starting new work, aru reads `.aru/transaction.toml` and digest-gates a deterministic rollback to the complete old state. Dry runs refuse to continue while recovery is pending.

If a destination or backup contains unknown manual changes, recovery stops and preserves both content and journal. Copy the affected project file and `.aru/transaction.toml` before manual repair. Do not delete backups until you understand whether each digest represents old or new state.

## Audit integrity

Run a detailed local review without network or writes:

```console
aru audit
aru audit --format json --output audit.json
```

Audit checks manifest/lock consistency, pending recovery, ownership references, projection drift, deployed skill content, cached plugin tree and manifest digests, and hidden Unicode format controls.
It exits non-zero when blocking findings exist.
