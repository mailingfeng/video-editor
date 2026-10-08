<!-- BEGIN COMPOUND CODEX TOOL MAP -->
## Compound Codex Tool Mapping (Claude Compatibility)

This section maps Claude Code plugin tool references to Codex behavior.
Only this block is managed automatically.

Tool mapping:
- Read: use shell reads (cat/sed) or rg
- Write: create files via shell redirection or apply_patch
- Edit/MultiEdit: use apply_patch
- Bash: use shell_command
- Grep: use rg (fallback: grep)
- Glob: use rg --files or find
- LS: use ls via shell_command
- WebFetch/WebSearch: use curl or Context7 for library docs
- AskUserQuestion/Question: present choices as a numbered list in chat and wait for a reply number. For multi-select (multiSelect: true), accept comma-separated numbers. Never skip or auto-configure — always wait for the user's response before proceeding.
- Task (subagent dispatch) / Subagent / Parallel: run sequentially in main thread; use multi_tool_use.parallel for tool calls
- TaskCreate/TaskUpdate/TaskList/TaskGet/TaskStop/TaskOutput (Claude Code task-tracking, current): use update_plan (Codex's task-tracking primitive)
- TodoWrite/TodoRead (Claude Code task-tracking, legacy — deprecated, replaced by Task* tools): use update_plan
- Skill: open the referenced SKILL.md and follow it
- ExitPlanMode: ignore
<!-- END COMPOUND CODEX TOOL MAP -->

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->

## 发布版本

用户说“发布版本”时，默认将本次发布的代码提交、合入并推送至 `main`，然后发布以下两个平台，无需再次询问平台：

| 平台名称 | 原生目标 | 发布附件 |
| --- | --- | --- |
| `windows_x64` | `x86_64-pc-windows-msvc` | NSIS 安装器 `.exe` |
| `mac_intel` | `x86_64-apple-darwin` | 安装镜像 `.dmg` |

若用户明确指定平台、版本号或发布范围，遵循当次要求。默认沿用当前发布通道；内测版递增 beta 编号，以 GitHub Pre-release 发布。`package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 和 `src-tauri/tauri.conf.json` 的应用版本必须一致。

使用 `.github/workflows/windows-x64.yml` 与 `.github/workflows/macos-intel.yml` 在对应原生 runner 构建同一个已合入 `main` 的提交。等待两边的测试、构建、安装资源检查和安装后媒体回归全部通过，核对产物的源提交与版本，再创建版本标签并公开 GitHub Release。

每次发布须同时包含两个平台的安装包、`SHA256SUMS`、平台构建与安装资源记录、验证日志、桌面验收说明及第三方材料说明。检查上传后的附件完整性和校验和，最终报告版本号、Release 链接以及 `main` 的提交。

测试视频、临时媒体和本地凭据不得加入提交或发布附件。内测包的公开发布不等于正式发行验收通过；未完成的签名、公证或桌面验收应按实际证据记录。
