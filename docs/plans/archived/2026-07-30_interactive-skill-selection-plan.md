# Interactive Skill Selection Plan

## Goal

讓 `aru skill add <source>` 在互動式終端中先探索 Git source 的 skills，再用可搜尋的多選選單讓使用者決定要安裝哪些項目；同時新增 `--all` / `-a` 作為明確安裝全部 skills 的非互動選項，並保持 lock、transaction、ownership 與 deterministic manifest contracts。

## Context

- `src/cli.rs` 目前把沒有 `--skill` / `--path` 的 add 解讀為 wildcard。
- `src/app.rs::skill_add` 在解析 arguments 後立即把 `include` 設為 `["*"]`，尚無 TTY 或取消流程。
- `src/skill.rs::discover_and_select` 已能 bounded discovery，但 discovery 與 requirement filtering/digest materialization 綁在一起，無法先列出完整候選。
- `src/resolver.rs` 擁有 Git ref resolution、cache checkout 與 conservative lock reuse；選單 preview 必須重用這些規則，避免顯示與實際 lock 不同的 revision。
- `aru.toml` 已用 explicit `include` 表示固定選擇，用 wildcard `include = ["*"]` 表示現在及未來所有有效 exports。

## Architecture

- `src/interactive.rs` 擁有 TTY 判斷、`inquire::MultiSelect` 呈現、空選擇 validation，以及 cancel/interruption mapping；其他模組不直接依賴 terminal UI。
- `src/skill.rs` 將 bounded candidate discovery 與 requirement selection/digest 分離；選單只顯示經 `validate_name` 驗證的 stable skill names，不渲染不受信任的 frontmatter description 或 path。
- `src/resolver.rs` 提供單一 source inspection，沿用 canonicalization、SemVer/revision resolution、conservative lock reuse及cache limits，回傳 exact resolved version/revision與排序後 candidates。正式 resolve 接受並驗證這個 pre-resolved hint，確保 prompt 後即使 tag 移動也只安裝使用者看過的 revision。
- Interactive add 採 inspect → prompt → commit：等待輸入時不持有 project operation lock；commit 前重新取得 lock，重讀並比對 `aru.toml` / `aru.lock` snapshot，若其他 process 已修改 project 就 fail closed並要求重試。
- `--skill`、`--path`、`--all` 都是明確 selector並完全跳過選單；只有三者都缺少時才進入 interactive mode。

## Tech Stack

- Runtime dependency：`inquire = { version = "0.9.4", default-features = false, features = ["crossterm", "fuzzy"] }`。
- TTY detection：標準庫 `std::io::IsTerminal`，要求 stdin 與 inquire 使用的 stderr 都是 terminal。
- Unix PTY integration test：target-specific dev dependency `expectrl = "0.9"`；selection policy與non-TTY tests保持跨平台。

## Assumptions

- Bare add 在 stdin/stderr 都是 TTY 時開啟選單；在 pipe、CI或redirect環境中於任何 network或project write前失敗，訊息要求使用 `--all` / `-a`、`--skill` 或 `--path`。
- `--all` / `-a` 代表 wildcard intent：寫入 `include = ["*"]` 並清空 `exclude`；它與 `--skill`、`--path`互斥。
- 新 source 的選單預設不勾選；至少要選一項才能確認。手動選取全部仍保存 explicit names，只有 `--all` 才追蹤未來新增的 exports。
- 已存在的 explicit source 預先勾選目前 include；確認後以畫面上的完整集合取代 explicit selection。已存在的 wildcard source 預先全選；全部維持勾選時保留 wildcard，取消任一項時轉為 explicit selection。
- Existing `paths` 中仍可解析的 skill 會出現在選單；保留勾選就保留 path mapping，取消勾選就移除該 mapping。
- Esc 是成功取消並不更動 manifest、lock、state或agent projections；Ctrl-C/terminal I/O error回傳清楚的非零錯誤。Inspection 可能留下可安全回收的 immutable `.aru/cache` shard。
- `--dry-run` 在 TTY 中仍顯示選單並依選擇列出 plan，但不寫入 project；在非TTY中仍要求明確 selector。

## Non-Goals

- 不建立 full-screen Ratatui application、theme system或自訂 event loop。
- 不改造 `skill remove`、`skill update` 或 MCP commands為互動式流程。
- 不讓 bare non-TTY add 繼續隱式安裝全部；既有 automation 必須遷移到 `--all`。
- 不從 untrusted skill內容執行preview、Markdown rendering或description ANSI輸出。

## Risks

- **Automation breaking change**：bare non-TTY command將失敗；以短旗標 `-a`、長旗標 `--all`、help與README migration說明降低成本。
- **Prompt/resolve漂移**：remote tag可在選單期間移動；inspection exact revision必須被正式resolver驗證及重用，不能重新按tag偷偷選另一commit。
- **Concurrent project mutation**：prompt期間其他 aru process可能完成寫入；commit前比較manifest/lock snapshots，差異時保留對方內容並中止。
- **Terminal corruption或prompt hang**：只在真TTY啟動 inquire，Esc/Ctrl-C paths必須恢復terminal；PTY tests覆蓋confirm與cancel。
- **大量exports**：沿用既有 discovery limits，使用fuzzy filter與固定page size；truncated或oversize discovery仍fail closed而非顯示partial list。
- **語意混淆**：選單選取全部是explicit snapshot，`--all`才是wildcard；prompt help與README必須清楚區分。

## Plan

- [x] 更新 `Cargo.toml` / `Cargo.lock` 加入最小feature集合的 `inquire 0.9.4` 與Unix-only `expectrl 0.9` dev dependency，並以 `cargo tree -e features -i inquire` 證明未啟用未使用的editor/date/one-liner features。 Evidence: `cargo tree -e features -i inquire`只列出`crossterm`、`fuzzy`與其`fuzzy-matcher`依賴。
- [x] 更新 `src/cli.rs::SkillAddArgs` 加入 `-a` / `--all`，把 `all`、`skills`、`path` 放入同一互斥selector group並更新help文字；以CLI tests證明short/long flags可解析且與`--skill` / `--path`衝突。 Evidence: focused help/conflict與long/short wildcard integration tests通過。
- [x] 在 `src/interactive.rs` 建立純selection policy與 `inquire::MultiSelect` adapter，實作stdin+stderr TTY gate、stable name options、fuzzy/page help、existing defaults、non-empty validator、Esc cancel與Ctrl-C/error mapping；以unit tests注入fake chooser驗證confirm、empty、cancel及error結果而不依賴真terminal。 Evidence: `cargo test --lib interactive::tests`通過3項mode、stable/default、confirm/empty/cancel/error tests。
- [x] 重構 `src/skill.rs`，新增bounded deterministic candidate inventory並讓既有`discover_and_select`透過共同inventory做selector filtering與digest materialization，包含existing explicit paths；以現有security tests加上candidate order、path preservation及all-candidates boundary tests證明重構不放寬limits。 Evidence: `cargo test --lib skill::tests`通過10項discovery、path、symlink、size與portable-boundary tests。
- [x] 在 `src/resolver.rs` 新增單一skill source inspection與validated pre-resolved hint，沿用canonical source、previous lock、version/rev、cache和digest規則；以temporary Git tests證明normal preview保守重用locked revision、explicit update intent可選新revision、moved tag在prompt後仍使用previewed SHA，identity/requirement不符的hint會fail closed。 Evidence: focused resolver test與`cargo check --all-targets --all-features`通過。
- [x] 重整 `src/app.rs::skill_add` 為explicit selector fast path與interactive inspect/prompt/commit path，實作snapshot revalidation、existing selection defaults、wildcard-preservation/explicit-conversion、path mapping cleanup以及transactional manifest/lock/sync；以application-level fake chooser tests證明取消與concurrent snapshot mismatch不寫manifest、lock、state或projections。 Evidence: `cargo test --lib app::tests`通過cancel、concurrent edit與explicit/wildcard replacement三項application tests。
- [x] 擴充 `tests/cli.rs`：non-TTY bare add在fetch前失敗、`--all`與`-a`寫wildcard並同步、`--skill` / `--path`不開prompt、TTY multi-select只安裝勾選項、TTY Esc取消、TTY dry-run不寫檔；Unix互動案例用`expectrl`和local Git fixture驗證實際inquire鍵盤流程及terminal正常退出。 Evidence: `cargo test --test cli`通過16項tests，含4項真PTY confirm/filter/Esc/Ctrl-C/dry-run paths。
- [x] 更新 `README.md` quick start、offline examples與manifest selection semantics，說明bare TTY menu、搜尋/方向鍵/Space/Enter/Esc、explicit snapshot與`--all` wildcard差異、CI migration及non-TTY error；以README command review及`aru skill add --help` snapshot確認文件和CLI一致。 Evidence: generated help包含`-a, --all`與所有selector/dry-run flags，`git diff --check`通過。
- [x] 執行 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-targets --all-features`、`just check`與`git diff --check`，並手動在Linux terminal對一個public multi-skill GitHub repository完成一次選取及一次Esc取消；所有local gates與smoke evidence成功才完成。 Evidence: `just check`通過51 unit與16 CLI tests（public smoke預設ignored），`git diff --check`通過；另以`cargo test --test cli public_interactive_git_select_and_cancel_smoke -- --ignored --exact`在PTY完成`narumiruna/skills@0.5.0`的writing-plans選取與Esc取消。

## Completion Checklist

- [x] `aru skill add owner/repo` 在真TTY中顯示名稱排序穩定、可搜尋且至少選一項的multi-select，完成後manifest/lock/projections只包含選取的skills。 Evidence: local PTY test先驗證empty validator，再filter/select beta並斷言manifest、lock與projection只有beta。
- [x] `aru skill add owner/repo --all` 與 `-a` 都跳過prompt並保存wildcard intent；與`--skill` / `--path`混用時在fetch或write前失敗。 Evidence: long/short local integration test與Clap conflict tests通過。
- [x] Bare add在non-TTY中快速失敗並提供可執行的migration訊息，不會隱式安裝全部或等待輸入。 Evidence: non-TTY test使用未fetch的GitHub shorthand並斷言error、manifest unchanged、no lock/cache。
- [x] Esc、Ctrl-C、empty selection、terminal error及concurrent project mutation都不留下partial manifest/lock/state/projection transaction，且terminal恢復可用。 Evidence: real PTY Esc/Ctrl-C/empty tests、fake terminal error test與concurrent snapshot mutation application test全部通過。
- [x] Interactive preview和最後lock使用同一canonical source與exact revision；prompt期間moved tag或變更project inputs不會導致silent drift。 Evidence: resolver temporary-Git test在moved tag後以validated hint鎖回preview SHA，invalid descriptor及concurrent manifest change fail closed。
- [x] Existing explicit、wildcard、exclude與custom path requirements在重新開啟選單時有可預測的defaults及保存/轉換結果。 Evidence: application tests驗證explicit replacement、unchanged wildcard preservation、exclude-to-explicit conversion及custom path keep/remove。
- [x] Discovery limits、cache integrity、lock replay、`--locked`、`--dry-run`、ownership adoption/drift與non-interactive explicit selectors的既有tests持續通過。 Evidence: full 51-unit/16-CLI default suite通過，interactive lock另以`sync --locked` replay。
- [x] README、CLI help、unit/PTY/integration tests、quality gates及public interactive smoke一致證明功能完成。 Evidence: `just check`、`git diff --check`與ignored-on-default public PTY select/cancel smoke均通過。
