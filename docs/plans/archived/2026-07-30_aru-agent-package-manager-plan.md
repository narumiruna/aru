# aru Agent Package Manager 初步實作計畫

## Goal

建立一個 Rust CLI，讓專案以 `aru.toml` 宣告 coding agents、Agent Skills 與 MCP servers，以 `aru.lock` 鎖定可重現的來源版本與 agent projection，並以 `aru sync` 安全同步至各 agent 的 project paths。

MVP 支援：

- Codex skills：`.agents/skills/`
- Claude Code skills：`.claude/skills/`
- Codex MCP：`.codex/config.toml`
- Claude Code MCP：`.mcp.json`

> 使用隱藏目錄 `.agents`、`.claude`；不是 `agents`、`claude`。

## Context

目前 repository 只有最小 Rust binary：`Cargo.toml` 無 dependencies，`src/main.rs` 只輸出 `Hello, world!`。先固定資料格式、CLI contract、失敗復原與安全邊界，再逐步實作 resolver 與 adapters。

沿用 Cargo/uv 的核心心智模型：

- `aru.toml`：人維護的 requirements、selectors 與 agents。
- `aru.lock`：aru 產生且應提交的精確 commit、版本、內容摘要、MCP transport/package selection 與 portable projection baseline。
- `.aru/state.toml`：不提交的本機 deployment state；只記錄實際 projection mode、last-applied digest 與 transaction metadata，不取代 lock。
- `aru skill update` / `aru mcp update`：明確解鎖全部或指定 package；一般 `aru sync` 保留仍相容的 locked entries。
- `aru sync --locked`：lock 缺少、stale，或 agent capability projection 不完整時失敗，不修改 lock。
- `aru sync`：必要時補齊 lock，但不因遠端有新版本而升級已相容的 locked package。
- MVP 不提供 uv-style `--frozen`；避免部署已知與 manifest 不一致的 lock。未來若加入，必須另外設計其安全語意。

已由 reference implementations 驗證的方向：

- Agent Skills 是至少含合法 `SKILL.md` 的目錄。
- Codex 會從 repository `.agents/skills` discovery，且 project/repo scope 支援 directory symlink。
- Vercel Skills 已採 `.agents/skills` canonical copy，再投影至 agent-specific paths。
- Cargo/uv 都會把既有 lock 當 preference，只有 update 目標被解鎖。
- Codex 與 Claude MCP schema 不同，transport/auth capability 也可能不同，必須有共通 domain model、per-agent capability check 與 renderer。
- MCP Registry 仍是 preview；API/schema 必須隔離，且 response、pagination 與候選選擇需有明確上限和決策規則。

本地查詢索引：`third_party/reference/references.md`。

## Proposed User Experience

```console
# 初始化並記錄 agents
aru init --agent codex --agent claude-code

# 加入 collection 中全部合法 exports；解析最新相容 SemVer tag
aru skill add narumiruna/skills

# 以穩定 skill name 選擇一個或多個 exports
aru skill add narumiruna/skills --skill writing-plans
aru skill add narumiruna/skills --skill writing-plans --skill applying-tdd

# path 只作為非標準 layout 或同名衝突的 escape hatch
aru skill add narumiruna/skills --path skills/writing-research/writing-plans

# 版本/ref 使用獨立 flag，避免與 git@host SCP-like source 混淆
aru skill add narumiruna/skills --version 0.5.0 --skill writing-plans
aru skill add ssh://git@example.com/team/skills.git --rev 67cd354 --skill writing-plans

# 升級全部或單一來源；其他 locked packages 保持不變
aru skill update
aru skill update narumiruna/skills

# 移除整個來源，或只移除該來源的一個 export
aru skill remove narumiruna/skills
aru skill remove narumiruna/skills --skill writing-plans

# 加入 MCP Registry server；ambiguous transport/package 必須明確選擇
aru mcp add io.example/context7 --name context7
aru mcp add io.example/context7 --name context7 --transport stdio --package-registry npm

# 同步 lock 狀態
aru sync
aru sync --locked
aru sync --dry-run

aru mcp remove context7
```

`add` 採 uv-like 行為：成功時以同一個 recoverable transaction 更新 `aru.toml`、`aru.lock` 與 projections。`--no-sync` 仍解析並更新 manifest + lock，只跳過 agent projection；它不留下刻意 stale 的 lock。`--dry-run` 可做解析與網路讀取，但不得修改 manifest、lock、`.aru/` cache/state 或 agent paths，只輸出 deterministic operation plan。

非互動模式不顯示 selector 選單：只有 source 時加入全部合法 exports；可用重複的 `--skill` 或單一 `--path` 限縮。任何歧義都失敗並列出可用選項。

### Source 與 Skill Selector Grammar

Positional argument 永遠只代表 package source，不包含 skill path 或 version/ref：

- **主要語法**：`aru skill add <source> --skill <skill-name>`。
- **路徑 fallback**：`aru skill add <source> --path <repo-relative-path>`；該目錄必須直接含合法 `SKILL.md`。
- **整包安裝**：`aru skill add <source>`。
- **版本/ref**：`--version <SEMVER-REQ>`、`--rev <COMMIT>`；未來再加入 `--tag`、`--branch`、GitHub `/tree/<ref>/<path>` input sugar。

`--version` 與 `--rev` 互斥。MVP 不解析 `<source>@<version>`，避免與 `git@host:path`、URL userinfo 或含 `@` 的 repository 名稱混淆。

不採 `owner/repo/subpath` 作主要語法：它只對 GitHub shorthand 容易切割，對完整 Git URL、GitLab、self-hosted Git、SSH 與 local path 都不一致。source parser 不理解 skill selector；selector 在 source 被 fetch 並驗證後才套用。

## Proposed Manifest

```toml
schema = 1

[project]
agents = ["codex", "claude-code"]

[skills]
# 裸版本採 Cargo caret 語意；=0.5.0 才是 exact requirement。
"narumiruna/skills" = { version = "0.5.0", include = ["writing-plans"], exclude = [] }

# 使用 --path 時無條件保存 name-to-path override：
# "owner/repo" = { version = "1.0.0", include = ["review"], paths = { review = "extras/review" } }

[mcp.context7]
registry = "https://registry.modelcontextprotocol.io"
server = "io.example/context7"
version = "1.2.0"
# 省略時只允許唯一且所有 agents 都支援的 candidate；否則 fail ambiguous。
transport = "stdio"
package-registry = "npm"

# Direct remote 沒有可升級版本。
[mcp.internal-docs]
transport = "streamable-http"
url = "https://docs.example.com/mcp"
bearer-token-env = "DOCS_MCP_TOKEN"
```

語意：

- Git skill source 的 `version = "0.5.0"` 是 `^0.5.0` requirement；`=0.5.0` 才是 exact。`rev` 與 `version` 互斥。
- GitHub `owner/repo` canonicalize 成無 credential 的 HTTPS identity；fetch 可沿用 credential helper。完整 SSH source 保留必要的 user/host，但 lock 與 diagnostics 不保存或顯示 embedded password/token。
- `include = ["*"]` 代表全部合法 exports；`exclude` 只對 wildcard 有效。`aru skill remove <source> --skill X` 在 wildcard 模式加入 `exclude`，在 explicit 模式移除 `include` 與對應 `paths`；explicit list 變空時移除 package entry。
- 多次 `add --skill X` 在 explicit 模式做 set union；wildcard 模式則從 `exclude` 移除 X。相同 canonical source 不建立第二份 package entry。
- `--skill` 保存穩定 name；實際 path 鎖在 `aru.lock`。新版只移動目錄但 name 不變時仍可 update。
- 凡使用 `--path`，manifest 一律保存 `paths.<name>`，直到使用者明確移除 override；不能只依賴 lock 暫存這項 intent。
- MCP server version 採同一套 SemVer requirement；Registry 的 non-SemVer version 只接受 `=<literal>` exact requirement，不參與 latest/ordering。
- Registry 同版本有多個 package/remote 時，`transport` / `package-registry` 是可攜式 selector。沒有 selector時只有唯一候選才能繼續，禁止依 API array order 選第一個。
- manifest、lock、state、generated project config、diagnostics 與 snapshots 都只能保存 secret env name，不能保存 secret value。aru 不讀取 secret value；若 host 不支援 runtime env reference，該 target fail closed。
- project scope 是 MVP 唯一 scope。

## Proposed Lockfile

`aru.lock` 使用 machine-generated TOML，概念欄位如下：

```toml
version = 1
package-input-hash = "sha256:..."
projection-input-hash = "sha256:..."

[[skill-package]]
source = "git+https://github.com/narumiruna/skills.git"
requirement = "0.5.0"
version = "0.5.0"
revision = "67cd354cc2eeb417db200a4f8d78869b03a0753d"
skills = [
  { name = "writing-plans", path = "skills/writing-research/writing-plans", sha256 = "sha256:..." },
]

[[mcp-server]]
name = "context7"
registry = "https://registry.modelcontextprotocol.io"
server-id = "io.example/context7"
version = "1.2.0"
metadata-sha256 = "sha256:..."
targets = [
  { agent = "codex", kind = "package", transport = "stdio", package = { registry = "npm", identifier = "...", version = "1.2.0" } },
  { agent = "claude-code", kind = "package", transport = "stdio", package = { registry = "npm", identifier = "...", version = "1.2.0" } },
]

[[projection-baseline]]
agent = "codex"
kind = "skill"
key = "writing-plans"
sha256 = "sha256:..."

[[projection-baseline]]
agent = "claude-code"
kind = "mcp"
key = "context7"
sha256 = "sha256:..."
```

規則：

- Git tag 只負責版本選擇；lock 一律保存完整 commit SHA。一般 sync 直接 fetch/checkout locked SHA，不重新解析 floating tag。
- 每個 materialized skill 保存來源相對 path 與 canonical content digest；不需要再 hash 整個未選 repository。
- MCP 鎖 exact server metadata version、normalized metadata digest，以及每個 agent 實際選中的 package/remote transport。新增 agent 不應升級 package version，但可能需要補一個 target selection。
- `package-input-hash` 涵蓋 skill/MCP requirements、selectors、path overrides 與 normalized source identity，不含 agents。
- `projection-input-hash` 涵蓋 package lock identity、排序後 agents、adapter capability schema version 與會改變 renderer selection 的欄位。只變更 agents 時 package lock可保留，但 `--locked` 必須在 target selection 不完整時失敗。
- `projection-baseline` 是可提交、可重建的 ownership bootstrap：state 遺失時，只有 destination 與 baseline semantic digest 完全一致才能自動 adopt；不同內容仍視為 collision。baseline 只描述目前 desired entries，不能授權刪除不再宣告且沒有 state 證明的舊 entry。
- metadata digest 針對 aru normalized selected fields 做 canonical serialization，不 hash 受 key order、pagination wrapper 或無關 preview extension 影響的原始 JSON bytes。
- lock entries、skills、targets、baseline 與 map keys固定排序，確保 deterministic diff。

## Architecture

資料流：

```text
aru.toml
   │ parse + validate
   ▼
Resolver ── Git source / MCP Registry
   │ preserve compatible locked entries
   ▼
aru.lock (exact versions, revisions, digests, per-agent selections)
   │ fetch + verify into immutable content-addressed cache
   ▼
.aru/cache/
   │ stage + recoverable transaction
   ▼
.agents/skills/                 # canonical installed skill bytes; Codex reads directly
   └── ClaudeSkillAdapter ──> .claude/skills/ (relative symlink or verified copy)

Agent MCP adapters
   ├── Codex  ──> .codex/config.toml
   └── Claude ──> .mcp.json
```

建議模組邊界：

- `cli`：解析命令與輸出，不持有 resolution/sync policy。
- `manifest`：`aru.toml` typed model、validation、保留格式的 edits。
- `lockfile`：exact resolved model、stable serialization、兩層 input hash 與 stale check。
- `source::git`：source canonicalization、SemVer tag listing、exact revision fetch；永不理解 skill selector。
- `source::mcp_registry`：隔離 preview API/schema、pagination、limits 與 canonical normalization。
- `skill`：bounded discovery、Agent Skills validation、selector 與 canonical digest。
- `resolver`：保留 compatible lock、selected-package update、agent capability selection 與 conflict detection。
- `cache`：`.aru/cache/git/<source-hash>/<revision>/` immutable shards、per-shard lock、digest verification、same-filesystem atomic landing。
- `ownership`：portable baseline bootstrap、本機 state、collision/drift/adoption policy。
- `transaction`：project operation lock、operation plan、sibling staging、journal、backup、commit/rollback/recovery。
- `agent::{codex, claude}`：pure capability declaration與 desired config/skill projection renderer；不下載、不解析版本、不直接 commit writes。
- `sync`：把 resolver、cache、ownership、adapters 與 transaction 串成 desired-state reconciliation。

不要讓 adapter 自己下載 package，也不要讓 resolver 直接寫 agent 設定。所有 adapter 先產生可驗證的 desired entries，再由 transaction layer 統一套用。

### Skill Discovery and Digest

MVP discovery roots：

1. source root 本身含 `SKILL.md`；或
2. `skills/**/SKILL.md`；或
3. `--path <repo-relative-directory>` 指向直接含 `SKILL.md` 的單一目錄。

安全與資源界線：

- `--path` 不接受 absolute path、`..`、`.`、空 segment、NUL、Windows prefix/drive/UNC；normalize 後必須仍在 package root。
- Conventional scan 只走 `skills/`，最大 depth 6、2,000 directories、20,000 entries；達任一上限即 fail truncated，不安裝 partial result。
- `SKILL.md` 最大 1 MiB；selected skill 單檔最大 10 MiB、總 regular-file bytes 最大 100 MiB。常數集中定義並在 diagnostics 顯示實際 limit。
- MVP 不 follow 或 materialize source symlink；selected skill 內任何 symlink、device、socket、FIFO 或其他特殊 entry 都失敗。這比允許「安全的 internal symlink」更容易保持跨平台 bytes一致，未來可另行擴充。
- 拒絕非 UTF-8 relative path、case-fold 後重複 path，以及 Windows reserved names；避免同一 lock 在不同 filesystem 投影成不同 tree。
- `SKILL.md` 的 `name` 必須合法且與 parent directory 相同；同一 package 或 resolved set 中 duplicate name 失敗。
- 不執行 skill 內腳本；CLI 在 add/update 前顯示 source、version、skills 與 digest diff。

Canonical skill digest 對排序後每個 regular file依序 hash：format version、normalized `/` relative path length/path、executable-bit marker、content length/raw bytes。所有整數使用固定 big-endian encoding；不使用不帶 delimiter 的字串串接。materialization 後重新計算同一 digest，與 lock 不符即失敗。

### MCP Resolution and Capabilities

在凍結 v1 schema 前先完成 bounded spike，回答：

- `/v0.1/servers` opaque cursor、all-versions 與 exact-version endpoint 的實際 response/status 行為。
- non-SemVer version、deprecated/deleted status、同版本多 packages/remotes 的選擇與 ordering。
- npm/PyPI/Cargo/OCI/MCPB package metadata 能否轉成不經 shell 的 command + args。
- Codex 與 Claude 對 stdio、streamable HTTP、SSE、headers、bearer env、stdio env reference 的真實 project-scope支援。

MVP policy：

- Custom registry 預設只允許 HTTPS；拒絕 URL userinfo。HTTP 只可由未來明確 insecure opt-in 開啟，MVP 可先不提供。
- Connect/read timeout、最多 redirects、10 MiB response body、最多 100 pages 與 10,000 version records；超限或 pagination cycle fail closed。
- JSON/schema/decode error 不 fallback 成空 registry，不接受 partial/truncated inventory。
- package/remote selection 先套 manifest selector，再套所有 agents capability；剩餘不是唯一候選時 fail ambiguous。
- Registry runtime command與 args 全程使用 process argument arrays，不經 shell。Lock 保存 exact package version、runtime、args template 與 env-name requirements所需的 normalized fields。
- adapter capability 無法安全表示某 auth/env reference 時，整個 sync 在寫入前失敗；不得讀 env value後內嵌至 config。
- Codex inline bearer token 一律拒絕，只能使用 `bearer_token_env_var` / env-backed headers。Claude 的 env interpolation 能力以 spike 結果為準，未驗證前不宣稱與 Codex 共通。

### Projection and Ownership

- `.agents/skills/<name>` 是 canonical installed bytes；Codex 不需要額外 symlink。Claude 預設建立指向 canonical directory 的 per-skill relative symlink；不支援 symlink時 copy並驗證 digest。
- `.aru/state.toml` 記錄 destination、kind、mode、last-applied semantic digest 與 owning lock identity。`.aru/` 加入 `.gitignore`；`.agents/`、`.claude/` 與 agent configs 不自動忽略，使用者可選擇提交 projections。
- state 遺失時，當現有 skill bytes或 MCP entry semantic digest與 `projection-baseline` 完全一致，可 bootstrap adoption；不同內容預設 collision。沒有 state 且不在 current baseline 的 entry永不自動刪除。
- skill copy以 canonical tree digest判斷 drift；symlink以 link target + canonical tree digest判斷。MCP 以單一 server entry 的 normalized semantic digest判斷，不以整份 config formatting判斷。
- remove 只刪除 current state證明 aru 擁有且仍等於 last-applied value的項目。已被人工修改時 fail drift並保留。
- 同名 unmanaged skill/MCP entry預設失敗。`--force` 是明確且 destructive 的 takeover：preview 必須顯示會取代的 path/key；commit 後舊值不保證在未來 remove 時自動恢復。
- MCP renderer只 merge/remove aru-owned server keys，保留其他 keys、comments與 entries。既有 TOML/JSON 無法 parse 時 fail closed，絕不重寫成空 config。
- 任何 destination ancestor symlink都先 resolve並驗證仍在 project root；escaping symlink、final unmanaged symlink或 path type不符時失敗。寫檔使用 sibling temp、nofollow/metadata re-check與 atomic rename，防止 option/path/symlink injection。

### Transaction and Concurrency

單一 mutating command 的流程：

1. 取得 project operation lock；在 lock 內重新讀 manifest、lock、state與 destinations。
2. 完成解析、fetch、digest、collision/drift與所有 adapter validation，產生 deterministic operation plan。
3. 在各 destination 同一 parent filesystem建立 sibling staging；寫入 durable transaction journal，記錄 old/new digest、backup與目前 phase。
4. 依固定順序 replace destinations；每步更新 journal。最後寫 `aru.lock`、`aru.toml` 與 `.aru/state.toml` 的順序由 recovery protocol固定，不以「最後寫 state」取代 journal。
5. 全部成功後標記 committed、移除 backups/journal，再 garbage-collect未引用 cache shard。

每個 mutating command開始前先檢查 journal：若 transaction未 committed，依 journal與 destination digests deterministic rollback或 roll-forward；無法安全判定時停止並提供 recovery diagnostics，不猜測 ownership。SIGKILL/power loss 無法提供瞬間 all-or-nothing visibility，但下一次 aru invocation 必須可恢復到完整 old 或 new state。

## Rollback / Recovery

- Resolution、schema、digest或 collision validation失敗發生在 apply 前，不得留下 manifest/lock/projection變更。
- Apply 中一般錯誤觸發 immediate rollback，恢復已 replace 的 destinations；rollback本身失敗則保留 journal與 backups供下次啟動繼續。
- Crash recovery只操作 journal中列出的、且 old/new digest可證明的 paths；未知或人工修改內容一律保留並停止。
- `.aru/cache` shard永遠 immutable；partial shard使用 `.incomplete-*` sibling staging，下次 access在 shard lock內清理。
- `.aru/state.toml` 被刪除時可由 lock baseline與 exact destination match重建 current ownership，但不能重建已移除 package的歷史 ownership或自動刪除未知 orphan。
- `aru sync --dry-run` 顯示 create/update/remove/adopt/collision/drift與 lock diff，作為人工 recovery前的預覽。

## Tech Stack

實作前以最小 spike驗證 API，避免預先建立不必要 abstraction：

- CLI：`clap`
- data model：`serde`, `toml`, `serde_json`
- TOML round-trip edits：`toml_edit`
- SemVer：`semver`
- diagnostics：先選 `thiserror` + `miette` 或 `anyhow` 其中一條主路徑，不並存兩套 public error model
- HTTP：`reqwest` + rustls，啟用 streaming body cap、timeout與 redirect policy
- integrity/filesystem：`sha2`, `walkdir`, `tempfile`, `directories`
- locking：以最小 cross-platform spike比較 `fs2`/`fd-lock` 或等價 crate後再決定
- Git MVP：`std::process::Command` 參數陣列呼叫 system `git`，使用 `--`/validated positional arguments避免 option injection，沿用 SSH/credential helper；之後再依 standalone binary需求評估 `gix`
- tests：unit + temporary repositories；CLI integration用 `assert_cmd`；HTTP以 local mock server/recorded fixtures，不把 live network放進一般 test suite

Quality gates：

```console
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## Non-Goals for MVP

- 自建 skills registry、搜尋排行榜或 publishing service。
- transitive skill dependencies、workspace 或 feature groups。
- 自動執行 skills/MCP package 中的 install scripts。
- global/user scope、enterprise policy與 agent自動偵測。
- uv-style `--frozen`、完整 offline mode與跨機器共享 global cache。
- 支援所有 coding agents；先以 Codex/Claude 驗證 adapter capability差異。
- 管理 Claude/Codex OAuth credential storage，或把 secret value寫入 generated config；登入仍交給 host CLI。
- 完整 Claude/OpenAI plugin package；第一版只管理標準 skills與 MCP declarations。
- 允許 source skill tree內 symlink或特殊 filesystem entry。

## Assumptions

- `aru skill add <source>` 動態追蹤該版本全部合法 exports；單一移除以 `exclude` 保存 intent，新出現的其他 exports在 update後仍會加入。
- `narumiruna/skills` 的 tags 可作 collection版本；已確認存在 `0.0.1` 到 `0.5.0` 等 SemVer tags，`0.5.0` 對應 commit `67cd354...`。
- MVP 需要 system `git`，讓 private source沿用既有認證。
- `.agents/skills` 是 canonical deployment；`.aru/cache` 只作 immutable fetch/materialization cache。
- `aru.toml`、`aru.lock` 應提交；`.aru/` 可重建且不提交。Agent projections可提交，state缺少時以 exact baseline adoption重建。

## Unknowns to Resolve Early

- Live MCP Registry 是否完全遵循 checkout內 OpenAPI的 pagination、status與 exact-version contract。
- Claude Code目前對 `.mcp.json` runtime env interpolation、env-backed HTTP headers與 stdio env references的官方支援；若不能安全表示，MVP必須對該 target拒絕相關 server。
- Windows建立 project symlink/junction的權限與 parent symlink canonicalization；copy fallback需維持相同 digest語意。
- Git source沒有 SemVer tags時，是否只接受 `--rev`，或加入明確 `--branch/--head`；MVP優先採 fail並要求 explicit ref。
- Registry stdio package managers首版支援集合；由 spike決定 npm、PyPI/uvx、Cargo、OCI中哪些有足夠 deterministic且跨-agent的 renderer。

## Risks

- **惡意 skill / prompt injection**：安裝前顯示來源、版本、skill清單與 digest diff；不執行內容；文件要求 review。
- **供應鏈漂移**：lock exact commit/package version與 canonical digest；fetch後不符即失敗。
- **資源耗盡**：bounded repository walk、file sizes、HTTP body、pagination、timeouts與 concurrency；truncated result fail closed。
- **破壞既有 agent設定**：portable baseline + local state、collision fail、entry-level merge、journal recovery；未知內容不刪除。
- **中途失敗或並行 sync**：project operation lock、sibling staging、durable journal、digest-gated rollback/roll-forward。
- **Registry breaking change或候選順序漂移**：preview schema隔離；ambiguous selection失敗；lock只保存 stable normalized model。
- **secret洩漏**：aru不讀 secret values；URL/userinfo與 error context redaction；manifest/lock/state/config/log/snapshot只允許 env name。
- **跨平台不一致**：canonical path/digest、source symlink拒絕、relative symlink + copy fallback、Windows/Unix fixtures。
- **TOML/JSON巨大 diff**：manifest/Codex用 round-trip mutation；Claude做 semantic entry merge；parse failure不重寫；golden tests限制改動。

## Plan

- [x] 先完成 MCP/agent capability bounded spike，對 local OpenAPI fixtures與可選 live read-only requests驗證 Registry pagination/version/package/remote行為及 Codex/Claude env/auth transports；產出 `docs/spikes/` 決策記錄與 mock fixtures，且所有未知 capability都有 support或fail-closed結論。
- [x] 定義 `aru.toml` v1、`aru.lock` v1、`.aru/state.toml`、transaction journal與 canonical digest的 schema/fixture contracts，包含兩層 input hash、per-agent MCP selections、projection baseline、`include`/`exclude`/`paths`；以 round-trip、stable-order與 golden tests證明相同 semantic input產生相同 bytes/hash。
- [x] 建立 `clap` command tree（`init`, `lock`, `sync`, `skill add/remove/update`, `mcp add/remove/update`）與 application boundary，固定 `--version`/`--rev`、per-skill remove、`--locked`/`--dry-run`/`--no-sync` grammar；以 help snapshots與 invalid/ambiguous argument integration tests驗證。
- [x] 實作 manifest discovery、typed parsing、validation與 `toml_edit` mutation，使 add/remove保留無關 comments/keys，並正確合併 wildcard/exclude/path intent；以 fixture diffs證明每個命令只改預期 tables。
- [x] 實作 Git source canonicalization、credential-redacted identity、SemVer tag resolution、exact revision fetch與 conservative lock reuse，且 subprocess不經 shell或接受 option injection；以 temporary bare remotes證明 normal sync不升級、named update只解鎖目標、moved tag仍由 locked SHA保護。
- [x] 實作 bounded skill discovery、name/path selectors、frontmatter validation、source-entry rejection與 canonical digest，採 depth/entry/byte limits且 truncated fail；以 nested collection、explicit path、duplicate name、oversize、symlink、special entry、case collision與malicious path fixtures驗證。
- [x] 實作 content-addressed `.aru/cache` shards與 `.agents/skills` canonical materialization，使用 per-shard lock、same-parent staging、post-copy digest verification與 copy fallback；以 concurrent fetch、partial shard、cache corruption、Windows/Unix path fixtures證明不暴露 incomplete content。
- [x] 實作 portable projection baseline與 `.aru/state.toml` ownership engine，涵蓋 exact-match adoption、collision、drift、remove與destructive `--force` preview；以 state遺失、committed projection、manual edits及unknown orphan filesystem tests證明只操作可證明的owned entries。
- [x] 實作 project operation lock、durable transaction journal、sibling backups與啟動時rollback/roll-forward recovery，覆蓋 manifest、lock、skills、MCP configs與state；以每個apply phase注入失敗及模擬crash的integration tests證明下次 invocation恢復完整old或new state。
- [x] 實作 Codex/Claude skill adapters，使 Codex直接使用 canonical `.agents/skills`、Claude使用安全relative symlink或verified copy；以兩個agent directory trees、parent symlink escape與copy fallback snapshots驗證。
- [x] 依 spike決策實作 bounded MCP Registry client、stable domain model、version/selector resolver與per-agent capability selection；以 pagination cycle、oversize body、non-SemVer exact、ambiguous candidates、unsupported transport與metadata canonicalization tests驗證fail-closed行為。
- [x] 實作 Codex `.codex/config.toml` 與 Claude `.mcp.json` pure render/merge adapters，只修改owned server entries、保留unrelated config並拒絕parse error或unsupported secret reference；以 comments/keys preservation、collision、manual drift、inline secret rejection與capability matrix golden tests驗證。
- [x] 串接 `aru lock`、transactional `aru sync`、`--locked`、`--dry-run`及 add/update/remove的預設sync/`--no-sync`流程；以乾淨temporary project與state-loss checkout做Codex+Claude end-to-end，證明package lock保守重用而agent變更只補projection selection。
- [x] 補齊 `README.md` quick start、manifest/lock semantics、generated/committed files policy、security limits、secret model、agent capability matrix與journal recovery手冊；以本地fixtures逐一執行文件中的無外網範例並保存預期輸出。
- [x] 執行 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-targets --all-features`，並在Linux做真實public Git smoke test；所有local gates與smoke test成功才視為MVP完成。

Execution evidence (2026-07-30): all three local quality gates pass; temporary-repository unit/integration suites cover the contracts above; public smoke tests installed `narumiruna/skills@0.5.0` at commit `67cd354cc2eeb417db200a4f8d78869b03a0753d` and Registry package `agency.kesey/pretrip@1.0.1`, then replayed both with `sync --locked` for Codex and Claude Code.

## Completion Checklist

- [x] `aru skill add narumiruna/skills`、`--skill writing-plans`、`--path ...`與`--version 0.5.0`皆更新manifest/lock並同步；explicit path無條件保存在manifest。
- [x] `aru skill remove <source> --skill <name>`在explicit與wildcard/exclude模式都保持可預測，且不移除同來源其他skills。
- [x] `aru skill update [source]`與`aru mcp update [name]`具conservative update行為，未選packages不意外升級。
- [x] 變更`project.agents`不升級package版本，但會使缺少的per-agent projection selection在`--locked`下失敗、一般sync下被補齊。
- [x] `aru sync --locked`能在乾淨checkout重建相同skill bytes與MCP semantic entries；missing/stale lock失敗，state缺少但exact baseline可安全adopt。
- [x] 每個transaction apply phase失敗或crash後，下次aru invocation可恢復完整old或new state；unknown/manual content不被rollback、overwrite或delete。
- [x] Discovery、Git與Registry limits有boundary tests，任何truncated/oversize/cycle/schema error都fail closed而非partial install。
- [x] MCP candidate ambiguous或agent不支援transport/auth-env時在寫入前失敗，且不依API array order偷偷選擇。
- [x] Secret values不出現在`aru.toml`、`aru.lock`、`.aru/state.toml`、transaction journal、generated project configs、CLI diagnostics、logs或test snapshots。
- [x] Codex/Claude官方project paths、canonical `.agents/skills`與至少一個兩者都安全支援的MCP transport各有end-to-end test。
- [x] `aru sync`、remove、state-loss adoption與failure recovery均不覆蓋或刪除非aru管理的skills/MCP entries。
- [x] 所有quality gates與public Git smoke test通過，README足以讓新使用者完成init、add、sync、update、remove與recovery。

## References

- Local implementation lookup: `third_party/reference/references.md`
- Agent Skills specification: https://agentskills.io/specification
- Codex skills: https://developers.openai.com/codex/skills
- Codex MCP: https://developers.openai.com/codex/mcp
- Claude Code skills: https://code.claude.com/docs/en/skills
- Claude Code MCP: https://code.claude.com/docs/en/mcp
- MCP Registry: https://modelcontextprotocol.io/registry/about
- uv locking and syncing: https://docs.astral.sh/uv/concepts/projects/sync/
