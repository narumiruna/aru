# Lock and sync

Aru separates dependency resolution from target projection so teams can review and commit exact state before replaying it elsewhere.

## Command behavior

| Command | Behavior |
| --- | --- |
| `aru lock` | Resolve instructions and packages; update `aru.lock` without changing target paths |
| `aru lock --check` | Check that the existing lock is complete and current without writing or network access |
| `aru sync` | Reuse compatible locked packages, fill missing lock data, and reconcile target paths |
| `aru sync --locked` | Require a complete current lock; never update it or advance a branch |
| `aru sync --check` | Check the lock and every target path locally without writing |
| `aru sync --dry-run` | Print the deterministic plan without changing persistent project state |

A practical team workflow is:

```console
# After changing aru.toml or package requirements
aru sync

git add aru.toml aru.lock
git commit

# In CI or on another machine
aru sync --locked
aru sync --check
```

Do not use blanket staging when target projections may include local-only files; review the plan and stage intended paths.

## Preview and deferral

`--dry-run` may read Git or HTTP sources through temporary storage, but it does not modify `aru.toml`, `aru.lock`, `.aru/`, or target paths.

Mutating add, remove, and update commands accept `--no-sync`. They still resolve and transactionally update manifest and lock intent, skip target projections, and print the command required to apply them later.

## Offline and frozen operation

Common global controls work across commands:

```console
aru --offline sync
aru --locked sync
aru --frozen sync
```

- `--offline` disables remote Git and Registry access.
- `--locked` fails if the command would change `aru.lock`.
- `--frozen` combines `--locked` and `--offline`.

For committed, previously cached state, `aru --frozen sync` provides the strongest replay constraint.

## Inspect state

Use read-only inspection commands to understand the lock:

```console
aru tree
aru tree --depth 2 --target claude
aru tree --invert shared-rules
aru info agent-kit
aru metadata --format-version 1
```

Run an integrity audit for detailed local findings:

```console
aru audit
aru audit --format json
```

`sync --check` is the concise exact-state gate. `audit` additionally checks manifest and lock consistency, pending recovery, ownership references, projection drift, deployed skill content, and hidden Unicode format controls.

## Deterministic output

List and machine-readable data go to stdout; human-readable status goes to stderr. Normal status uses Cargo-style verbs:

```text
      Locked skill review 1.2.0
     Created skill review (.agents/skills/review)
     Updated aru.lock
    Finished Project synchronized.
```

Use `-v` when exact revisions and digests are needed, `-q` to suppress routine status, and `--color never` for plain logs.
