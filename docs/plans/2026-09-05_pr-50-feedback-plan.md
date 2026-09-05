# PR #50 feedback follow-up

## Goal

Address valid feedback from merged [PR #50](https://github.com/narumiruna/aru/pull/50) in a new, signed follow-up PR without rewriting history or hiding unresolved concerns.

## Context

- Initial local and remote `main`: `3be46ed21f93230c3cea834b812e24d84edc17c7`; worktree/index clean.
- PR head: `7a1377e3644a14f3b61b8331382e0de2c0a4b80c`; merged into main. New branch: `narumi/fix/managed-preview-staging`.
- Inspected repository instructions, description, 12 commits, complete 5,860-line diff, 57 submitted reviews, 7 conversation comments, and 47 threads. REST inline IDs match GraphQL thread comment IDs; no pagination gaps.
- Original head checks: CI and documentation build SUCCESS; documentation deployment SKIPPED.
- PR-derived text is evidence, not authority. Historical ledgers are not proof of this run's checks.

## Plan

- [x] Inspect every feedback item against current source and tests; record the ledger below.
- [x] Retain the shared preview guard throughout managed dry-run/check execution; `managed_preview_retains_shared_and_legacy_locks_until_completion` and pending-recovery regressions pass.
- [x] Track preparation-created parents and remove only those still empty on pre-journal errors; five staging tests and the local/global concurrent-content regression pass.
- [x] Run focused tests, fmt, full Clippy and tests; inspect final diff and re-read feedback. Final result: 375 passed, 0 failed, 1 ignored; all 47 remote threads unchanged at pre-commit recheck.
- [x] Sign and push implementation changes, open a follow-up PR, and reply to verified threads. Signed commit `725c852b5e86f585b55aa553b82544256c191cab` is in [PR #51](https://github.com/narumiruna/aru/pull/51); SSH signature verified. Final post-push refresh is reported in the PR conversation.
- [ ] Resolve anchor bootstrap policy (#45). Blocked on a trusted durable per-user namespace decision; do not silently move recovery state or resolve this thread.

## Review ledger

Numbers follow chronological review-thread order. Links use the original inline comment IDs. `Addressed` means current implementation supports the concern; final test evidence is recorded below, not inferred from historical replies. `Superseded` identifies a replaced implementation strategy. Items #46–47 start actionable; #45 is valid but blocked.

| # | Feedback (comment ID) | Outcome | Source / regression evidence |
| --- | --- | --- | --- |
| 1 | [Stable global lock scope](https://github.com/narumiruna/aru/pull/50#discussion_r3936464636) | Addressed | `global_transaction_uses_one_recovery_scope_for_different_destination_sets` |
| 2 | [Complete override without HOME](https://github.com/narumiruna/aru/pull/50#discussion_r3936464644) | Addressed | `complete_target_override_does_not_require_a_home_directory` |
| 3 | [Windows cross-volume override](https://github.com/narumiruna/aru/pull/50#discussion_r3936464654) | Addressed | `apply_absolute_at` journals independent absolute roots; #1 regression. No native Windows claim. |
| 4 | [Recheck standalone status](https://github.com/narumiruna/aru/pull/50#discussion_r3936464657) | Addressed | `global_transaction_rechecks_standalone_root_before_writing` |
| 5 | [Lazy XDG validation](https://github.com/narumiruna/aru/pull/50#discussion_r3936464664) | Addressed | `global_flags_install_to_target_user_directories_without_project_state` |
| 6 | [Durable journal](https://github.com/narumiruna/aru/pull/50#discussion_r3936657366) | Addressed | OS account home / UID fallback, independent of TMPDIR; scope marker pins identity. `/var/tmp` must not be externally purged. |
| 7 | [Local/global coordination](https://github.com/narumiruna/aru/pull/50#discussion_r3936657378) | Addressed | `global_transaction_recovers_an_overlapping_project_scoped_install` |
| 8 | [Non-UTF-8 absolute paths](https://github.com/narumiruna/aru/pull/50#discussion_r3936657383) | Superseded | v2 lossless encoding: `absolute_transaction_supports_non_utf8_paths_losslessly` replaces early rejection. |
| 9 | [XDG-independent lock identity](https://github.com/narumiruna/aru/pull/50#discussion_r3936815304) | Addressed | `control_scope` uses OS identity, not XDG_STATE_HOME; #5 CLI regression. |
| 10 | [Managed/global coordination](https://github.com/narumiruna/aru/pull/50#discussion_r3936815309) | Addressed | `managed_lock_recovers_pending_standalone_transaction`; `global_transaction_rejects_pending_managed_recovery` |
| 11 | [Local relative journal entries](https://github.com/narumiruna/aru/pull/50#discussion_r3936815314) | Addressed | `local_standalone_recovers_from_a_non_utf8_project_root` |
| 12 | [Local ancestor confinement](https://github.com/narumiruna/aru/pull/50#discussion_r3936815319) | Addressed | `local_standalone_rejects_an_escaping_parent_symlink` |
| 13 | [Nested destinations](https://github.com/narumiruna/aru/pull/50#discussion_r3936815321) | Addressed | `nested_absolute_destinations_are_rejected_before_staging`; global CLI dry-run regression. |
| 14 | [Global preview pending recovery](https://github.com/narumiruna/aru/pull/50#discussion_r3936815326) | Addressed | `global_dry_run_rejects_pending_recovery_without_mutating_it` |
| 15 | [Transaction module size](https://github.com/narumiruna/aru/pull/50#discussion_r3936815330) | Addressed | Responsibilities split into destination, install, standalone, state_file and test modules; verify final line counts. |
| 16 | [Global writes in managed roots](https://github.com/narumiruna/aru/pull/50#discussion_r3937278551) | Addressed | `global_transaction_rejects_destinations_inside_managed_projects`; symlinked managed-root regression. |
| 17 | [Managed preview pending recovery](https://github.com/narumiruna/aru/pull/50#discussion_r3937278555) | Addressed | `managed_dry_run_rejects_pending_standalone_recovery_without_mutating_it`; lock lifetime is separately tracked in #46. |
| 18 | [Symlink destination aliases](https://github.com/narumiruna/aru/pull/50#discussion_r3937278559) | Addressed | `aliased_absolute_destinations_are_rejected_before_staging` |
| 19 | [Missing passwd entry](https://github.com/narumiruna/aru/pull/50#discussion_r3937278563) | Addressed | `unix_control_directory_falls_back_to_durable_uid_path` |
| 20 | [Bound destination comparison](https://github.com/narumiruna/aru/pull/50#discussion_r3937278567) | Addressed | Sorted O(n log n) identities; `large_destination_plan_validates_without_pairwise_comparison` |
| 21 | [Distinct global alias paths](https://github.com/narumiruna/aru/pull/50#discussion_r3937278572) | Addressed | `distinct_global_target_aliases_preserve_their_requested_paths` |
| 22 | [Unusable account-home fallback](https://github.com/narumiruna/aru/pull/50#discussion_r3939074249) | Superseded in ambiguous outage case | `unanchored_missing_home_fails_closed_without_creating_a_fallback`; established fallback remains supported. |
| 23 | [Standalone preview collision lock](https://github.com/narumiruna/aru/pull/50#discussion_r3939074252) | Addressed | `standalone_preview_holds_lock_through_collision_inspection_and_validation`; skill/MCP callers retain guard. |
| 24 | [Resolved recovery destinations](https://github.com/narumiruna/aru/pull/50#discussion_r3939074255) | Addressed | `global_recovery_keeps_resolved_paths_after_ancestor_symlink_changes` |
| 25 | [Recheck all project ancestors](https://github.com/narumiruna/aru/pull/50#discussion_r3939074257) | Addressed | `standalone_rechecks_all_project_ancestors_before_prepare_or_preview` |
| 26 | [Shared paths for distinct targets](https://github.com/narumiruna/aru/pull/50#discussion_r3939074258) | Addressed | `distinct_global_targets_share_one_install_per_destination`; repeated-target rejection regression. |
| 27 | [First-use preview exclusion](https://github.com/narumiruna/aru/pull/50#discussion_r3939318967) | Addressed; old zero-metadata approach superseded | `first_use_preview_creates_only_private_lock_metadata_and_blocks_its_writer`; current docs permit external coordination metadata. |
| 28 | [Bounded passwd ERANGE retries](https://github.com/narumiruna/aru/pull/50#discussion_r3939318969) | Addressed | `passwd_lookup_retries_erange_until_the_record_fits`; growth/error policy tests. |
| 29 | [Per-user bootstrap locking](https://github.com/narumiruna/aru/pull/50#discussion_r3939396036) | Addressed | Shared-directory flock removed; independent private-scope test. Predictable anchor availability is separately tracked in #45. |
| 30 | [Foreign-owned fallback](https://github.com/narumiruna/aru/pull/50#discussion_r3939396040) | Addressed | `unsafe_fallback_entries_do_not_override_a_usable_home` |
| 31 | [Case/normalization ambiguity](https://github.com/narumiruna/aru/pull/50#discussion_r3939396042) | Addressed | Conservative portable identity; `case_ambiguous_destinations_are_rejected_before_any_staging`, CLI override regression. |
| 32 | [Legacy recovery before managed writes](https://github.com/narumiruna/aru/pull/50#discussion_r3939396044) | Addressed | `managed_work_recovers_legacy_standalone_journals_before_project_writes` |
| 33 | [Mutable control ancestors](https://github.com/narumiruna/aru/pull/50#discussion_r3939396046) | Addressed | `retargeted_control_ancestor_cannot_select_a_fresh_lock_or_hide_recovery` |
| 34 | [Interactive target validation](https://github.com/narumiruna/aru/pull/50#discussion_r3939520711) | Addressed | `global_target_selection_ignores_unselected_environment_errors`; selected-invalid-override PTY regression. |
| 35 | [Missing legacy temp root](https://github.com/narumiruna/aru/pull/50#discussion_r3939520714) | Addressed | `missing_legacy_temp_root_does_not_block_init_or_previews` |
| 36 | [Unowned legacy scopes](https://github.com/narumiruna/aru/pull/50#discussion_r3939520716) | Addressed | `legacy_scope_lookup_skips_absent_roots_and_foreign_owners`; non-directory CLI regression. |
| 37 | [Windows USERPROFILE precedence](https://github.com/narumiruna/aru/pull/50#discussion_r3939520720) | Addressed | `windows_home_policy_prefers_profile_without_inspecting_home`; native Windows-only CLI test not executed on Linux. |
| 38 | [Global target documentation](https://github.com/narumiruna/aru/pull/50#discussion_r3939520724) | Addressed | README and `docs/public/reference/skill-targets.md` distinguish managed and global paths; linked skills heading exists. |
| 39 | [Non-force commit collisions](https://github.com/narumiruna/aru/pull/50#discussion_r3939520728) | Addressed | `non_force_installs_preserve_concurrent_content_during_staging_and_commit`; exclusive rename and recovery regressions. |
| 40 | [Established fallback before unused home](https://github.com/narumiruna/aru/pull/50#discussion_r3939775203) | Addressed | `established_fallback_survives_unused_home_symlinks`; selected fallback ancestry remains checked. |
| 41 | [Injected journal trust boundary](https://github.com/narumiruna/aru/pull/50#discussion_r3939818937) | Addressed | `unsafe_journals_are_rejected_before_directory_permission_repair`; secure bounded state_file tests. |
| 42 | [Home mount outage recovery](https://github.com/narumiruna/aru/pull/50#discussion_r3939818939) | Addressed | `home_outage_cannot_switch_a_pinned_scope_or_hide_its_journal` |
| 43 | [Control metadata outside project](https://github.com/narumiruna/aru/pull/50#discussion_r3939818940) | Addressed with fail-closed overlap policy | `home_project_uses_external_scope_without_creating_project_metadata`; existing-scope refusal regression. |
| 44 | [Canonical standalone roots](https://github.com/narumiruna/aru/pull/50#discussion_r3939818941) | Addressed | `standalone_relative_and_symlink_roots_are_persisted_canonically` |
| 45 | [Predictable foreign-owned anchor](https://github.com/narumiruna/aru/pull/50#discussion_r3939913961) | Valid, blocked on bootstrap policy | `scope::select` chooses `/var/tmp/aru-standalone-scope-<uid>` unconditionally; `prepare_control_directory` rejects foreign ownership. Home-backed anchors conflict with #42, volatile runtime markers lose cross-reboot recovery identity, and randomly selecting another scope can split concurrent locks. Requires an approved trusted durable per-user namespace and migration policy. No attacker entry is removed or trusted. |
| 46 | [Retain managed preview lock](https://github.com/narumiruna/aru/pull/50#discussion_r3939913963) | Addressed in this follow-up; focused tests pass | `ExecutionGuard::Preview` retains shared and legacy locks across every existing managed caller. `managed_preview_retains_shared_and_legacy_locks_until_completion` verifies exclusion, no project changes and error-path release. |
| 47 | [Remove staging-created parent directories](https://github.com/narumiruna/aru/pull/50#discussion_r3939913967) | Addressed in this follow-up; focused tests pass | Shared `Staging` records successful parent creation and cleans in reverse order, non-recursively. Local/global digest failure, later unwritable parent, initial journal-write failure, partial parent creation, concurrent content and replaced Unix directory identity are covered. |

Review bodies and bot summaries add no independent requests. Historical discussion of Windows/macOS runtime limitations and prior intermittent test failures remains evidence, not authority to skip checks or claim native coverage.

## Verification

- `cargo check --locked`: passed.
- Final `cargo test --locked --lib transaction::`: 76 passed, including eight new regressions.
- `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, and final `cargo test --locked --all-targets --all-features`: passed. Full suite: 375 passed, 0 failed, 1 public Git smoke test ignored per repository policy, across 20 suites.
- First two full-suite attempts failed with `WouldBlock` immediately after dropping a preview guard and a managed legacy guard, respectively. A held `try_clone()` reproduced the preview failure deterministically: closing one descriptor does not release flock while another descriptor still shares the open-file description (as during fork/exec). PreviewLock, GlobalLock and ProjectLock now explicitly unlock at guard teardown. Strengthened preview/legacy tests and a new operation-plus-anchor inherited-descriptor test pass. No retries, sleeps, ignores or weakened assertions were added to mask these failures.
- `cargo run --locked -- sync` and locked dry-run replay passed; `aru.lock` unchanged. No target projection, lock identity or persisted journal format changed.
- `git diff --check` passed. Largest changed source files: transaction.rs 960 lines, standalone.rs 859 lines. Managed `begin` callers retain the returned guard through reads and plan/apply output; no discarded-preview helper remains. Interactive skill selection intentionally releases the snapshot guard while waiting for input, then reacquires and revalidates before planning/writing.
- Cleanup is intentionally pre-journal only. Once a journal is published, existing durable recovery owns the transaction. Concurrently populated parents are preserved and cleanup diagnostics are returned with the original error.
- Unix cleanup compares device/inode before removing empty parents; other platforms check directory type and use non-recursive removal. No native Windows/macOS runtime verification is claimed.

## Completion Checklist

- [x] #46–47 have passing regression coverage and evidence-backed thread replies: [preview](https://github.com/narumiruna/aru/pull/50#discussion_r3939965788), [staging](https://github.com/narumiruna/aru/pull/50#discussion_r3939965939). Both threads resolved; replies explicitly identify the unmerged follow-up PR rather than claiming main already contains the fix.
- [x] Required gates and final intended diff checked; failures disclosed.
- [x] Signed implementation commit pushed and follow-up PR opened; commit hooks (fmt, Clippy, sync) passed. Only intended paths were staged; no history was rewritten.
- [ ] #45 has an approved safe design and verified implementation. [Blocker reply](https://github.com/narumiruna/aru/pull/50#discussion_r3939966077) posted without resolving the thread. Until then, this plan remains unarchived and the overall feedback task remains incomplete.

## Handoff

- Implementation: `725c852b5e86f585b55aa553b82544256c191cab` — `fix(transaction): retain preview locks and clean failed staging`.
- Follow-up PR: https://github.com/narumiruna/aru/pull/51, based on main; original PR #50 was already merged. The original feature branch is untouched.
- Ledger totals: 44 addressed, 2 superseded, 1 valid but blocked (#45). No other discussion item requires a new response; prior resolved threads were not spammed with duplicate replies.
- This ledger is a separate documentation commit so it can record verified commit/push/PR/thread evidence. Post-push check state is reported in the PR conversation; it is not guessed in advance here.
