# AI Agent 持续迭代指南

> 2026-10-16，功能基线 `e8f5620`（自定义变化）。本文给出本仓库的工作入口与验证选择；命令清单不是本轮执行记录。

## 开始任务

1. 阅读 [AGENTS.md](../AGENTS.md) 与 [PROJECT.md](../PROJECT.md)，确认用户当次目标、改动范围和发布要求。
2. 查看 `git status --short`、当前分支与最近提交，识别用户已有修改。不要把未跟踪文件或参考素材自动纳入提交。
3. 只有根目录存在 `.codegraph/` 时，先用 CodeGraph 理解或定位代码；不存在则直接 `rg`。不要为满足文档流程自行建立索引。
4. 按下表阅读最少必要代码与测试，检查旧文档的日期、提交、平台和样本范围。新需求可以更新旧约束，但不能把未经验证的目标写成实现。
5. 明确变更要守住的 [架构不变量](../ARCHITECTURE.md)；对行为问题构造可观察的失败场景，再修改实现。

## 改哪里、看哪里、测什么

| 任务 | 首要入口 | 优先验证 |
| --- | --- | --- |
| 工作台布局、按钮、详情 | `ProcessingView.tsx`、`FileDetailsDialog.tsx`、`processing.css`、[DESIGN.md](../DESIGN.md) | 前端视图测试、类型检查、浏览器布局；文件选择与结果定位另做桌面验证 |
| 事件、重挂载、日志串行归属 | `useProcessing.ts`、`queueState.ts` | hook 和状态合并测试；乱序回包、旧 job 事件、切换选择、卸载 disposer |
| 输出目录记忆 | `useProcessing.ts` | `useProcessing.test.tsx`：选择后重挂载恢复、取消／迟到回包不覆盖、存储不可用；测试清理隔离的 localStorage |
| IPC 字段／命令 | `contracts.ts`、`desktop.ts`、Rust `contracts/`、`commands.rs`、`main.rs` | wire fixture、前端类型、desktop clippy，真实 Tauri 调用 |
| 文件夹收集、路径去重 | `media/inputs.rs`、`JobService` 导入方法 | `input_collection`、批次测试；Windows 大小写／别名、中文空格及晚到扫描 |
| 并发、取消、重置、退出 | `jobs/service.rs`、`jobs/state.rs`、`native/process.rs` | `batch_lifecycle`、`job_lifecycle`、`native_process`；重置同时检查视图／hook 的归零、重复操作、旧日志／事件隔离；再按媒体改动范围跑真实 lifecycle / batch |
| 预设或编码参数 | `presets/basic.rs`、相关预设、`media/validate.rs` | `basic_plan`、生成媒体、相关实样；保持时间轴、布局及画面／声音变化证据 |
| 探测、时间轴、色彩、音频兼容性 | `media/probe.rs`、`timeline.rs`、`validate.rs` | `input_probe`、`output_validation`、`pipeline_real`，覆盖对应边界样本 |
| 输出提交、恢复清理、历史 | `output/`、服务历史整理、`jobs/log.rs` | `output_workspace`、`user_reports`、生命周期测试；替换目录、符号链接、冲突与未知文件 |
| 许可 | `license.rs`、`license/tests.rs`、`useLicense.ts`、命令入口 | 截止前后、离线、慢网络、时钟回退、已启动批次继续与 UI 禁用 |
| sidecar、打包或安装升级 | `tools/sidecars.lock.json`、准备／校验脚本、Tauri 配置、`windows/`、两平台 workflow | Node 脚本测试、原生构建、安装资源、安装后媒体；Windows 安装升级脚本 |
| 仅上下文文档 | 本次涉及页面及对应源码 | 相对链接、路径、命令、版本与事实核对，`git diff --check`；不必为纯文档重新跑媒体或发布流水线 |

表中 TypeScript 文件位于 `src/features/processing/`（API 位于 `src/api/`），Rust 模块位于 `src-tauri/src/`，集成测试位于 `src-tauri/tests/`。不要把所有测试无差别重跑；扩大范围由新失败、未覆盖的风险或发布合同决定。

## 本地验证命令

常规自动检查，根据改动选择：

```sh
npm run test -- --run
npm run typecheck
npm run build
npm run test:sidecars
npm run test:release
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --features desktop -- -D warnings
```

单独跑测试目标，例如：

```sh
npm run test -- --run src/features/processing/queueState.test.ts src/features/processing/useProcessing.test.tsx
cargo test --locked --manifest-path src-tauri/Cargo.toml --test batch_lifecycle
cargo test --locked --manifest-path src-tauri/Cargo.toml --test wire_contract
```

真实媒体须先准备当前平台 sidecar：

```sh
npm run prepare:sidecars
npm run test:media
# 有本地研究原片时，才追加实样回归
npm run test:media -- --sample-dir docs/requirements/20261002-research
```

`test:media` 生成忽略的 `.media-fixtures/`，顺序执行 `pipeline_real`、`lifecycle_real`、`batch_real`、`custom_real` 中适用的 ignored 测试。指定 sample-dir 后查找 `原视频.mp4`；兼容性实样 `provided_compatible_media_end_to_end` 被脚本显式跳过，需要单独执行：

```sh
VIDEO_EDITOR_COMPAT_SAMPLE="$PWD/docs/requirements/20261002-research/h264.mp4" \
  cargo test --locked --manifest-path src-tauri/Cargo.toml --test pipeline_real \
  provided_compatible_media_end_to_end -- --ignored
```

不要因常规 `cargo test` 退出成功就声称真实媒体测试通过；ignored 用例不会自动运行。直接运行某个真实媒体目标时，需按其测试读取逻辑提供 fixture / sample 环境，不能假定它会自动生成：

| 环境变量 | 用途 |
| --- | --- |
| `VIDEO_EDITOR_FIXTURE_DIR` | 已生成媒体目录；完整脚本自动设置 |
| `VIDEO_EDITOR_SAMPLE_DIR` | 含研究原片的目录；脚本查找后传入 |
| `VIDEO_EDITOR_COMPAT_SAMPLE` | 指定兼容性视频文件，只读 |
| `VIDEO_EDITOR_INSTALLED_EXE` | 使用安装应用旁的工具执行原生回归；不是给应用 IPC 注入任意工具路径 |

环境配置参考 [development.md](development.md)。若共享 npm 缓存出现权限问题，使用 `npm ci --cache .superpowers/npm-cache`，不修改全局缓存权限。Apple Silicon 的 sidecar 缺失不能靠忽略校验或换成 PATH 工具来掩盖。

## 关键回归场景

涉及对应模块时，优先确认这些已有行为没有退化：

- 两项活动、一项等待；转换结束但校验仍在执行时不能补位。取消收尾期间同样不能提前补位。
- 一项损坏或失败不影响其他项；取消 waiting 不调用媒体 runner；关闭后不再启动新项，保存中的项等待完成。
- `job_snapshot` 先于映射到达、查询旧回包晚于进度到达、移除再导入、快速切换日志选择，都不串任务。
- VFR、无音轨、5.1／7.1、已知全范围、缺失颜色标记、不同音画起点及损坏容器得到明确结果；不放宽策略“让测试变绿”。
- 输入 SHA 不变；输出冲突不覆盖；陌生工作区文件不删除；临时脚本权限失败时停止任务。
- 用户报告不暴露命令、stderr 和完整参数；新版历史不持久化计划，旧记录保留真实时间和结果。

确定性调度测试复用 `src-tauri/tests/support/controlled_runner.rs` 的阻塞点；不要用固定 sleep 作为“发生了并发／完成了清理”的唯一证据。涉及原生进程时，再用真实媒体确认退出、清理和输出提交。

## 发布与证据

发布遵循根 [AGENTS.md](../AGENTS.md) 的完整合同，不在这里另立流程。五处应用版本必须一致：`package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 中本应用包版本、`src-tauri/tauri.conf.json`。不要误改依赖包版本。

Windows 与 macOS Intel workflow 构建同一个已合入 main 的提交；原生测试、构建、资源检查、安装后媒体全部通过后才打版本标签并公开 Release。Windows 安装逻辑另有 [模板说明](../src-tauri/windows/README.md) 与 `scripts/check-windows-upgrade.ps1`。macOS workflow 的 HTTP 429 备用资源恢复仅允许复用固定哈希的同版工具，更新工具锁定版本时同步审查备用来源。

CI 产出构建／验证附件，并不自动证明最终 Release 发布及附件校验完成。最终发布还需双平台安装器、`SHA256SUMS`、平台记录、验证日志、桌面验收说明与第三方材料说明。`verify-release.mjs --development` 允许开发验收范围，不能据此改写正式签名、公证或材料状态。

记录验证时至少包含：源码提交、应用版本、平台及工具来源、命令、结果、样本身份／哈希（如适用）、未执行范围。存量证据只证明其记录的提交；不要把 `docs/evidence/` 中的旧计数复制成本轮测试结果。平台审核另须关联实际输出文件与用户反馈；无法独立核验时注明来源。

## 知识维护与会话交接

| 变更触发 | 更新位置 |
| --- | --- |
| 能力、支持范围、默认预设、平台或版本基线变化 | [PROJECT.md](../PROJECT.md) |
| 生命周期、模块边界、wire、存储、安全边界变化 | [ARCHITECTURE.md](../ARCHITECTURE.md) |
| 信息架构、状态文案、禁用条件、token、响应式变化 | [DESIGN.md](../DESIGN.md) |
| 命令、测试前提、环境变量与验证路径变化 | 本文 |
| 新增长期入口文档 | [docs/README.md](README.md)，必要时更新根 AGENTS 导航 |
| 实验与外部反馈 | 对应 `docs/requirements/` 记录，保留失败和未验证结果 |
| 平台构建／安装或版本验证 | `docs/evidence/`、`docs/releases/` 和对应验收说明 |
| 代码无法充分表达的已解决且已验证教训 | 用 ce-compound 判断是否写入 `docs/solutions/`；一次一条，不能用普通进展充数 |

更新文档时保留可追溯的源码链接，标明核对日期／基线，区分“当前行为”“维护建议”“尚待验证”。不要把根 AGENTS 扩成研发日记，避免在多份上下文重复长清单；修正权威内容后让其他文档引用它。

跨会话交接至少留下以下信息，可放在相关任务计划或交接文档中；不必每次新增一套文件：

```text
目标与用户最新约束：
分支／基线／本次涉及文件（含用户原有修改）：
已完成行为与关键取舍（附源码／计划入口）：
验证命令、结果、平台与样本范围：
未完成／未验证项与下一步：
已同步的上下文／证据文档：
```

交付前检查相对链接、源码名称和 `git diff --check`，确认没有新增视频、临时媒体、凭据或构建产物。是否提交、推送、发布以用户授权与既有规则为准；单纯更新上下文文档不触发发布。

## 自定义变化回归入口

设置／冻结／草稿分别查 `custom_settings`、`custom_settings_store`、`custom_service` 及 `useCustomSettings.test.tsx`；媒体时钟查 `custom_timing`／`output_validation`。修改映射须同时跑 `custom_plan` 和 `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib presets::custom`，私有几何测试不会随集成目标自动运行。

`npm run test:media` 现在包含 custom_real 的四个 generated_ 场景（正反变速、音高与多声道尾音、布局／裁切／SAR、全部关闭／推荐／各项 9／全 9），batch_real 另外覆盖自定义并发、取消／退出、坏输入与输出冲突。音调用窗口频谱测量，避免振幅调制导致过零计数漏周期。仅普通 cargo test 通过仍不能声称这些 ignored 场景通过。

本轮证据与源码关联见[自定义变化验证记录](evidence/custom-variation-20261016.json)。参数公式在已确认规格，实际随机值不写入用户设置或记录；测试不得把完整计划序列化到文件。映射、处理算法或校验解释有调整时，在证据中留下依据与残余风险，不扩大完整性容差。
