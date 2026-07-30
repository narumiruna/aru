# Git Branch Skill Sources Plan

## Goal

讓使用者以明確的 `--branch <NAME>` opt-in 從 Git branch 安裝 skills，同時維持預設 SemVer tag resolution、exact-SHA lock replay、互動選單與conservative update安全語意。

## Architecture

- `aru skill add <source> --branch main` 與 `--version`、`--rev`互斥；未指定reference時仍解析最新matching SemVer tag。
- `aru.toml`移除既有`schema`欄位與validation，直接新增skill `branch` intent；目前屬初期開發階段，不做manifest schema versioning、migration或跨版本相容層。
- `aru.lock`維持version 1，以`requirement = "branch:<name>"`、`version = "<name>"`及exact 40-hex `revision`記錄branch resolution。
- Normal sync重用locked SHA；`skill update [source]`才重新查詢exact `refs/heads/<branch>`。`sync --locked`只materialize locked SHA。
- Branch preview與正式interactive apply沿用pre-resolved hint，因此prompt期間branch移動不改變使用者看到的內容。

## Non-Goals

- 不把bare add預設改成`main`，不自動猜remote default branch。
- 不讓`--version main`或`--rev main`混合SemVer、branch與commit語意。
- 不保證force-push後已不可達的舊commit仍能從remote重建；branch tracking是明確較弱的reproducibility opt-in。

## Risks

- **Moving refs**：lock永遠保存SHA；只有named update移動，preview hint防止TOCTOU。
- **Early-format compatibility**：依目前初期開發決策不使用manifest schema；README與format docs記錄branch欄位，但不承諾跨版本manifest相容性。
- **Ref injection/ambiguity**：以Rust驗證branch name並只查詢exact `refs/heads/<name>`；不經shell、不接受option-like或wildcard ref。
- **Force push**：README明示clean checkout可能無法fetch unreachable locked SHA，正式發布仍推薦SemVer tags。

## Plan

- [x] 更新`src/cli.rs`與CLI tests加入`--branch <NAME>`，和`--version`/`--rev`建立互斥reference group且不改bare defaults；以help/conflict tests驗證grammar。 Evidence: focused CLI help/conflict test先失敗後通過。
- [x] 更新`src/manifest.rs`、`docs/formats.md`與contract fixtures，完全移除manifest schema欄位/validation並加入optional branch，且version/rev/branch最多一個；以round-trip、unrelated unknown-key preservation與invalid combination tests驗證unversioned格式。 Evidence: `cargo test --lib manifest::tests`通過4項unversioned fixture、unrelated key preservation與reference exclusivity tests。
- [x] 更新`src/source/git.rs`加入strict branch validation與bounded exact-head resolution；以temporary bare/local remotes驗證nested branch、missing/malformed branch、option/wildcard injection及branch移動解析，並證明預設仍選SemVer tag。 Evidence: `cargo test --lib source::git::tests`通過6項branch parser/resolution、安全與SemVer tag tests。
- [x] 更新`src/resolver.rs`與`src/app.rs`傳遞branch intent、descriptor、lock reuse與pre-resolved hint；以unit/application tests驗證normal sync保守重用、interactive preview pin、`skill update`移到新head及其他packages不升級。 Evidence: resolver/app suites與focused branch CLI add/sync/locked/update tests通過；既有named update isolation test持續涵蓋untargeted packages。
- [x] 擴充`tests/cli.rs`與`README.md`，驗證`--branch main`的explicit與interactive add、unversioned manifest、exact lock、`sync --locked`、branch move/update、non-TTY selectors及force-push限制文件；執行local PTY與public `narumiruna/skills` main smoke。 Evidence: local explicit/PTY branch tests通過；ignored public PTY smoke從`narumiruna/skills` main選取`designing-user-experiences`並安全取消第二次prompt。
- [x] 執行`cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-targets --all-features`、`just check`及`git diff --check`；所有checks通過才完成。 Evidence: `just check`通過56 unit與18 default CLI tests（1 public smoke ignored），`git diff --check`通過；public main-branch PTY smoke另行通過。

## Completion Checklist

- [x] `aru skill add narumiruna/skills --branch main`顯示main內容並可選`designing-user-experiences`，而bare add仍使用SemVer tags。 Evidence: public PTY smoke成功選取該main-only skill；source test在branch前進後仍證明default解析1.0.0 tag SHA。
- [x] Branch manifest不包含或依賴schema欄位；version/rev/branch conflict或invalid branch在fetch/write前失敗。 Evidence: unversioned golden fixtures/新init assertions、CLI conflicts與invalid-branch no-write test通過。
- [x] Lock記錄branch intent與exact SHA；normal sync及`--locked`不移動，named update才前進到新branch head。 Evidence: local branch CLI test斷言`branch:live`、40-hex revisions、conservative sync/locked replay及named update前進。
- [x] Invalid、ambiguous、oversized或option-like branch/ref output fail closed，Git subprocess不經shell。 Evidence: strict branch-name tests與exact-ref parser malformed/duplicate/wrong-ref/record-limit tests通過；implementation使用`Command` argument arrays及bounded stdout。
- [x] Interactive branch preview遇到prompt期間branch移動仍安裝preview SHA，concurrent project mutation仍不被覆蓋。 Evidence: resolver moved-branch hint test鎖回preview SHA，既有application concurrent snapshot test通過。
- [x] README、format docs、fixtures、unit/integration/PTY tests、quality gates及public smoke一致證明功能完成。 Evidence: docs與unversioned fixtures已更新；56 unit、18 default CLI、public ignored PTY smoke及所有quality gates通過。
