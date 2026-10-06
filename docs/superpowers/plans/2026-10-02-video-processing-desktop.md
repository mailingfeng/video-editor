# Video Processing Desktop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现可追溯、可取消、输出经完整校验的单文件本地视频转换桌面应用，为后续样本处理链校准提供基础。

**Architecture:** React 界面通过受控 Tauri commands 调用 Rust；Rust 使用固定的 FFmpeg／ffprobe sidecar，负责探测、预设、任务、校验和保存。基础转换与未定位的样本滤镜分开管理，任务快照是界面状态的权威来源。

**Tech Stack:** Tauri 2、React、TypeScript、Vite、npm、Rust、Tokio、tokio-util、Serde、sha2、uuid；Vitest／Testing Library、Rust 单元与真实媒体集成检查。发行时使用各目标平台的 FFmpeg／ffprobe。

**Spec:** [已批准的桌面设计](../specs/2026-10-02-video-processing-desktop-design.md)；[样本分析报告](../../requirements/20261002-research/analysis-report.md)。执行者先读这两份文件。

## Global Constraints

- 首版目标为 Windows x64、macOS Apple Silicon 和 Intel，分别构建、分别验证。
- 输入契约为：MP4 中一个主视频轨，零或一个主音轨；8 bit SDR、恒定帧率、可输出 yuv420p 的偶数尺寸；音轨支持单声道或双声道。
- 输出为 MP4／H.264／AAC，保存独立文件，源文件保持只读。
- 首版同一时刻仅处理一个任务。
- libx264 Main、veryfast、yuv420p；禁用 B 帧，GOP 约十秒，禁用场景切换触发的额外关键帧；不硬编码 H.264 level。
- 视频源码率可取得且有效时使用其目标码率，否则使用 CRF 18；两种模式互斥。
- 有效源音频码率按 64～128 kbit/s 限定，缺失时用 96 kbit/s；音频使用 AAC、48000 Hz，保持支持的声道数。
- 视频时长偏差不超过一个输入帧；音轨时长允许差值为 `max(一个输入视频帧时长, 50 ms)`。
- 取消等待最多三秒，仍存活则终止并回收；已经进入提交或完成阶段的取消返回当前状态。
- 前端不能提交任意 FFmpeg 参数、滤镜文本或可执行路径，也不获得通用 shell 执行权限。
- 样本研究预设不纳入首版已验证预设；本地校验结果不命名为“平台去重成功”。
- 按仓库 AGENTS.md 在主线程顺序执行与复核，不派发 subagent；上方通用模板不覆盖此约束。实施使用 `superpowers:executing-plans`。

## Review Focus

1. 平均帧率相等但实际时间戳不均匀、帧数缺失：读帧核实或拒绝，不能误判 CFR。归任务 2。
2. 首帧 PTS 非零、音视频起点不同：保持相对时间关系，不靠强制帧率丢帧补帧。归任务 3、5。
3. 输出目录存在链接或旧任务标记、目标名被其他进程占用：拒绝覆盖，清理不越过本任务目录。归任务 4。
4. 子进程挂起、stderr 很多、stdout 在行中间分块：持续排空输出，取消后回收，日志有界。归任务 1、6。
5. 界面重新挂载后收到旧事件、重复订阅或重复点击开始：以更高版本快照为准，不重复任务或订阅。归任务 6、7。

---

## 执行前提、范围和文件结构

当前仓库只有设计与研究文档，无产品代码。本机为 macOS Intel，Node 24.11.0／npm 11.6.1／Rust 1.76；缺少 ffprobe。执行时准备兼容 Tauri 2 的 Rust 工具链，在工作树内记录 `rust-toolchain.toml` 与实际版本，不把当前旧 Rust 当作已验证环境。[Tauri 前置要求](https://v2.tauri.app/start/prerequisites/)

开始执行时按 `superpowers:using-git-worktrees` 建立隔离工作树。当前两个视频及 requirement.md 尚未跟踪，不会随新工作树出现；真实样本检查显式接收当前样本目录路径，保持只读，不把它们打包进应用。自动化回归使用生成的小测试媒体。

本计划只实现基础转换 MVP 与校准入口。额外色调／音频处理的完整复现需要下一份校准计划，不能因基础软件完成就标为已复现。分析报告已经交付，本计划不重新执行整套研究。

| 文件组 | 职责／创建任务 |
| --- | --- |
| `package.json`、`package-lock.json`、`index.html`、`tsconfig.json`、`vite.config.ts`、`vitest.config.ts`、`src/main.tsx`、`src/App.tsx` | 前端构建与应用入口／1，7 |
| `rust-toolchain.toml`、`.gitignore`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/build.rs`、`src-tauri/tauri.conf.json`、`src-tauri/src/main.rs`、`src-tauri/src/lib.rs` | 原生入口、构建、资源配置／1，7，8 |
| `src-tauri/src/contracts/{mod,media,plan,job,error}.rs`、`src/api/contracts.ts` | 数据契约，按领域分文件／1 |
| `src-tauri/src/native/{mod,tools,process}.rs`、`scripts/prepare-sidecars.mjs`、`tools/sidecars.lock.json` | 工具定位、子进程与可复核资源／1 |
| `src-tauri/src/media/{mod,probe,timeline,validate}.rs` | 输入探测、时间轴与输出校验／2，5 |
| `src-tauri/src/presets/{mod,basic}.rs` | 预设与处理参数／3 |
| `src-tauri/src/output/{mod,workspace,publish,recovery}.rs` | 临时目录、拒绝覆盖保存、残留识别／4 |
| `src-tauri/src/jobs/{mod,service,state,progress,log}.rs` | 单任务管理、状态、进度、受限日志／6 |
| `src-tauri/src/commands.rs`、`src/api/desktop.ts`、`src/features/processing/{ProcessingView.tsx,useProcessing.ts,processing.css}` | IPC 与用户处理流程／7 |
| `src-tauri/tests/`、`tests/fixtures/`、`src/features/processing/ProcessingView.test.tsx` | 行为、原生进程及媒体集成测试／对应任务 |
| `scripts/generate-fixtures.mjs`、`scripts/check-media.mjs`、`scripts/verify-release.mjs`、`docs/development.md`、`docs/release-verification.md` | 可重复测试、安装验证记录／7，8 |

`mod.rs` 只导出模块；桌面 crate 名为 `video-editor`，Rust 库名为 `video_editor`。衍生媒体、sidecar 二进制、构建目录和工作树目录由 `.gitignore` 排除，锁文件与工具来源清单跟踪。脚本均用 Node 的程序加参数列表 API，跨平台执行，不调用拼接的 shell 字符串。

### 固定的数据和命令约定

Rust IPC 数据用 Serde camelCase；状态与错误码用 snake_case。TypeScript 与 Rust 共享 `tests/fixtures/job-snapshot.json` 验证序列化契约。路径为字符串传入 IPC，原生层转 `PathBuf` 并核实；无法表达的路径返回错误，不能静默替换字符。

| 契约及所属文件 | 必需字段／类型 |
| --- | --- |
| `Rational`、`FileIdentity`，`contracts/media.rs` | `num:i64, den:u64` 且 den>0；identity 为 canonicalPath、sizeBytes、modifiedNs、sha256 |
| `VideoInfo`，同上 | codec、width、height、bitDepth、pixelFormat、frameRate／timeBase:Rational、frameCount:u64、startPts:i64、durationTicks:i64、bitRate:Option<u64>、可缺失的颜色范围／空间／primaries／transfer |
| `AudioInfo`，同上 | codec、sampleRate、channels、timeBase:Rational、startPts／durationTicks:i64、bitRate:Option<u64> |
| `MediaInfo`，同上 | identity:FileIdentity、container:String、video:VideoInfo、audio:Option<AudioInfo>、title／comment:Option<String> |
| `MetadataRequest`、`StartJobRequest`，`contracts/plan.rs` | 模式 preserve 或 override，override 的 title／comment 可缺失；request 为 inputPath、outputDirectory、presetId、metadata，不接受额外字段 |
| `ValidationPolicy`、`ProcessingPlan`，同上 | policy 为输出尺寸、帧率、帧数、视频／音频时长、声道、各自允许偏差；plan 为 jobId、source:MediaInfo、presetId／version、args:Vec<String>、workspace:OutputWorkspace、policy |
| `PresetSummary`，同上 | presetId、version:u32、title、evidenceStatus；首版仅公开 basic-transcode-v1 |
| `JobSnapshot`，`contracts/job.rs` | jobId、version:u64、state:JobState、progress:Option<f64>、startedAtMs／endedAtMs、outputPath／error 可缺失、cleanupPending:bool |
| `AppError`、`ErrorCode`，`contracts/error.rs` | code、message、可缺失 details；unsupported_input、damaged_media、tool_missing、output_permission、disk_full、process_exit、validation_failed、output_conflict、input_changed、busy、canceled、cleanup_pending |
| `ValidationResult`，`media/validate.rs` | outputPath、inputSha256／outputSha256、decodedFrames、videoDurationSeconds、audioDurationSeconds 可缺失、规格检查结果 |

任务 1 建立完整契约，包括 contracts/plan.rs 的 `OutputWorkspace`／`DirectoryIdentity` 数据形状（字段见任务 4），以及 contracts/job.rs 的 `JobState` 枚举（按规格的完整状态名称）。工作区行为由任务 4 实现，状态行为由任务 6 实现。时间戳判断使用有理数／整数，转换到秒只用于展示和允差比较。

### Task 1: 原生应用基础、工具资源与可回收的媒体进程

**Files:** 创建文件表中任务 1 的所有文件，另建 `src-tauri/tests/native_process.rs`、`src-tauri/tests/wire_contract.rs`、`src-tauri/tests/support/{mod,process_fixture}.rs`、`tests/fixtures/job-snapshot.json`。

**Interfaces:** 产出 `resolve_tools(manifest_dir:&Path, target:&str, bundled_exe:Option<&Path>)->Result<ToolPaths,AppError>`；`ToolPaths` 为 ffmpeg／ffprobe 两个固定路径。定义 `Tool::{Ffmpeg,Ffprobe}`、`RunSpec {tool:Tool,args:Vec<String>,stdoutPolicy:StdoutPolicy}`、`StdoutPolicy::{CaptureJson,Stream,Discard}`、`RunExit {exitCode:Option<i32>,stdout:String,stderrTail:String,canceled:bool}`、`ProcessEvent::{StdoutLine(String),StderrLine(String)}`。定义 `#[async_trait] trait MediaRunner:Send+Sync` 的 async `run(&self,spec:RunSpec,cancel:CancellationToken,events:mpsc::Sender<ProcessEvent>)->Result<RunExit,AppError>`；产出 `NativeRunner::new(paths:ToolPaths)->Self`。测试支持文件定义 ProcessFixture，提供 `new(mode:&str)->Self`、`paths(&self)->ToolPaths`、`is_alive(&self)->bool`，模拟程序可分块输出、挂起、输出大量 stderr 或返回非零。

- [ ] **Step 1 — 准备本任务的构建和测试入口。** 建立 React／Tauri 2 工程但保留现有 docs；锁定执行时选择的兼容依赖和工作树 Rust。定义 npm 的 `dev`、`build`、`test`、`typecheck`、`tauri`、`prepare:sidecars` 脚本；crate 加 Tokio process/io/time、tokio-util、async-trait、Serde、sha2、uuid，开发测试用 tempfile。先建立能够运行测试的最小 manifest。
- [ ] **Step 2 — 准备目标工具资源。** 取得同一构建版本的 FFmpeg／ffprobe：使用 FFmpeg 官方列出的构建来源或可复核的源码构建，核实版本、buildconf 和 libx264／AAC 可用性。实现 prepare-sidecars.mjs，把真实校验和、来源、构建配置及许可证写入 lock，再按 target triple 放入 binaries；后续准备必须与 lock 匹配，不能默默更新。此阶段只准备本机目标，其他目标在任务 8 补齐。[FFmpeg 下载入口](https://ffmpeg.org/download.html)
- [ ] **Step 3 — 写失败测试。** `wire_contract_roundtrip` 比较快照 JSON 回环且 state 为 running、version 为 3；`chunked_stdout_is_reassembled` 断言分块 UTF-8 得到完整一行；`stderr_flood_is_drained_and_truncated` 断言进程正常退出且尾部≤65536 字节；`hung_child_is_reaped_after_cancel` 断言 canceled 且子进程已退出；`unknown_target_or_missing_tool_is_error` 断言 tool_missing。

```rust
// native_process.rs::hung_child_is_reaped_after_cancel
assert!(exit.canceled);
assert!(!fixture.is_alive());
```

- [ ] **Step 4 — 运行失败测试。** Run: `cargo test --manifest-path src-tauri/Cargo.toml --test native_process --test wire_contract`。Expected: 因缺少类型或实现 FAIL，不能把工具缺失或安装错误算作行为失败。
- [ ] **Step 5 — 实现契约和 NativeRunner。** 发行路径按主程序同目录固定名称定位；开发路径读取 binaries 的 target 后缀，不回退搜索用户 PATH。用 `tokio::process::Command` 与参数列表执行固定工具，异步排空两条管道；JSON capture 上限 1 MiB，日志尾部 64 KiB，单行上限 64 KiB，事件通道容量 256。取消也能打断管道发送；接收者丢弃时回收子进程，不因满通道使取消卡住。FFmpeg 先写 `q`，最多三秒后 kill+wait；ffprobe 取消直接终止并回收；后台任务丢弃也不能遗留子进程。[Tauri 资源规则](https://v2.tauri.app/develop/sidecar/)
- [ ] **Step 6 — 验证接口和原生启动。** Run: 上述 cargo 命令、`npm run typecheck`、`npm run build`；Expected: 测试 PASS、构建退出 0，并在本机启动原生窗口验证两个工具自检。
- [ ] **Step 7 — 提交本任务的命名文件。** Commit: `feat: add desktop runtime and managed media tools`。不提交工具二进制或用户视频。

### Task 2: 输入探测、真实帧数和时间轴验证

**Files:** 创建 `src-tauri/src/media/{mod,probe,timeline}.rs`、`src-tauri/tests/input_probe.rs`、`src-tauri/tests/support/fake_runner.rs`、`tests/fixtures/probe/{valid,missing-frames,equal-rate-vfr,unknown-color,no-audio,hdr,multi-track}.json` 与匹配的时间戳文本；修改 lib.rs 导出模块。

**Interfaces:** 消费 `MediaRunner`。产出 async `probe_input(runner:&dyn MediaRunner,path:&Path,cancel:CancellationToken)->Result<MediaInfo,AppError>`、同步 `validate_input(info:&MediaInfo)->Result<(),AppError>`；测试文件定义 async `probe_fixture(name:&str)->Result<MediaInfo,AppError>`。support/fake_runner.rs 定义实现 MediaRunner 的 FakeRunner，用脚本化 stdout／退出状态注入探测结果，提供 `active_child_count(&self)->usize`；任务 6 在此文件补充屏障控制场景。

- [ ] **Step 1 — 写失败测试。** `missing_frame_count_reads_timestamps` 断言最终 frameCount=2713；`equal_rate_vfr_is_rejected` 断言 unsupported_input，即使两个平均帧率字段相等；`unknown_color_stays_unknown` 不制造 bt709；`audio_absence_is_preserved` 为 None；HDR、奇数尺寸、多主轨、超过双声道明确拒绝。

```rust
// input_probe.rs::missing_frame_count_reads_timestamps
assert_eq!(info.video.frame_count, 2713);
assert_eq!(info.video.frame_rate, Rational { num: 30, den: 1 });
```

- [ ] **Step 2 — 运行失败测试。** Run: `cargo test --manifest-path src-tauri/Cargo.toml --test input_probe`。Expected: 新能力未实现 FAIL。
- [ ] **Step 3 — 实现 probe/timeline。** 首次 ffprobe 使用 `-show_streams -show_format -of json`；使用实际 video stream index 排除附件图片轨。核实 CFR 必须读帧时间戳，不能只看 avg_frame_rate。第二次用指定视频轨的 `-show_frames -show_entries frame=best_effort_timestamp,duration -of compact=p=0:nk=0` 流式统计帧数、相邻 PTS 与最后一帧时长；未知、逆序或不均匀时间轴返回 unsupported_input，探测／解码失败为 damaged_media。采集轨道起点与 duration，而非用 mvhd 总时长代替。[ffprobe 文档](https://ffmpeg.org/ffprobe.html)
- [ ] **Step 4 — 运行并核实真实输入。** Run: 上述测试；用当前目标 ffprobe 只读探测原样本，确认 720×1280、30/1、2713 帧、44100 Hz。Expected: PASS 且输入哈希与报告一致。
- [ ] **Step 5 — 提交命名文件。** Commit: `feat: verify media inputs and frame timelines`。

### Task 3: 冻结基础预设和生成处理计划

**Files:** 创建 `src-tauri/src/presets/{mod,basic}.rs`、`src-tauri/tests/basic_plan.rs`；修改 contracts/plan.rs 补全构造与 lib.rs 导出。

**Interfaces:** 消费任务 1 的 `MediaInfo` 与 `OutputWorkspace` 契约，工作区文件操作在任务 4。产出 `list_presets()->Vec<PresetSummary>`、`build_plan(info:&MediaInfo,request:&StartJobRequest,workspace:OutputWorkspace,job_id:&str)->Result<ProcessingPlan,AppError>`。测试文件定义 `arg_value<'a>(args:&'a [String],name:&str)->Option<&'a str>`，不为测试增加产品访问器。

- [ ] **Step 1 — 写失败测试。** `missing_video_bitrate_uses_only_crf18`、`audio_bitrate_is_clamped` 覆盖低于 64k、高于 128k、缺失 96k；`no_audio_emits_no_audio_policy`；`nonzero_starts_keep_relative_timeline`；`metadata_quotes_are_one_argument`；`unknown_preset_is_rejected`。输入 30 fps 的 GOP 为 300，30000/1001 fps 为 300。

```rust
// basic_plan.rs::audio_bitrate_is_clamped
assert_eq!(arg_value(&low_audio_plan.args, "-b:a"), Some("64000"));
assert_eq!(arg_value(&high_audio_plan.args, "-b:a"), Some("128000"));
assert_eq!(arg_value(&missing_audio_plan.args, "-b:a"), Some("96000"));
```

- [ ] **Step 2 — 运行失败测试。** Run: `cargo test --manifest-path src-tauri/Cargo.toml --test basic_plan`。Expected: 缺少 build_plan 行为 FAIL。
- [ ] **Step 3 — 实现 build_plan。** 按 Global Constraints 生成 libx264／AAC 参数，GOP=round(10×fps)，-bf 0、-sc_threshold 0；不施加 -r、fps 或未知滤镜。用 copyts、fps_mode passthrough 保持原始时间关系，避免重置单独轨道起点；取约分后的输入视频 timeBase.den 为 MP4 video_track_timescale 整数，不用固定 15360 套用其他帧率。标题、备注和路径各自是一项参数，保留已知颜色信息。默认 comment 追加 `video-editor/basic-transcode-v1`；覆盖模式严格使用用户结构化值。冻结所有参数、输出政策和 preset version=1。
- [ ] **Step 4 — 运行测试和静态检查。** Run: 上述测试、`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`。Expected: PASS。真正的时间轴和转码行为在任务 5、7 的媒体检查中验证，参数单测不能替代它们。
- [ ] **Step 5 — 提交命名文件。** Commit: `feat: define reproducible basic conversion plans`。

### Task 4: 输出工作区、拒绝覆盖的保存和残留恢复

**Files:** 创建 `src-tauri/src/output/{mod,workspace,publish,recovery}.rs`、`src-tauri/tests/output_workspace.rs`；修改 contracts/plan.rs、Cargo.toml、Cargo.lock、lib.rs。

**Interfaces:** 产出 `prepare_workspace(output_dir:&Path,source:&Path,job_id:&str,record_dir:&Path)->Result<OutputWorkspace,AppError>`、`publish_no_replace(workspace:&OutputWorkspace)->Result<PathBuf,AppError>`、`cleanup_workspace(workspace:&OutputWorkspace)->Result<(),AppError>`、`recover_incomplete_jobs(record_dir:&Path)->Result<RecoverySummary,AppError>`。`OutputWorkspace` 含 jobId、directory、tempPath、finalPath、recordPath、directoryIdentity；`DirectoryIdentity {volumeId:String,fileId:String}` 是创建后取得的原生目录身份。`RecoverySummary` 含 cleaned／pending 任务 ID 与错误，不返回伪成功媒体。

- [ ] **Step 1 — 写失败测试。** `existing_destination_is_never_overwritten`、`unrelated_files_survive_recovery`、`symlink_workspace_is_not_followed`、`cleanup_failure_keeps_pending_marker`、`directory_swap_is_detected`。在任务目录创建后替换它的路径，保存和清理均拒绝操作新的目录；已有正式结果内容保持 sentinel。

```rust
// output_workspace.rs::existing_destination_is_never_overwritten
assert_eq!(error.code, ErrorCode::OutputConflict);
assert_eq!(std::fs::read(&workspace.final_path).unwrap(), b"sentinel");
```

- [ ] **Step 2 — 运行失败测试。** Run: `cargo test --manifest-path src-tauri/Cargo.toml --test output_workspace`。Expected: 尚未实现保存／恢复行为 FAIL。
- [ ] **Step 3 — 实现 workspace/publish/recovery。** 先 canonicalize 所选输出目录，再独占创建 `.video-editor-<jobId>`，临时媒体为 `output.mp4`，正式文件采用规格名称。AppData 的 job-records 保存任务、冻结参数、工作区身份和未完成标记，清理前核对标记、目录身份及边界；独占任务目录本身不接受链接或路径替换。macOS 用排他 rename，Windows 使用不替换目标的 MoveFileExW；只在同文件系统保存，不能使用普通可覆盖 rename 作为降级。平台 FFI 依赖按 cfg 配置在 Cargo.toml。无法提供这些语义时明确报错，保留未完成记录。[Apple 排他重命名说明](https://developer.apple.com/documentation/foundation/urlresourcevalues/volumesupportsexclusiverenaming)、[Windows 移动文件 API](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)
- [ ] **Step 4 — 验证保存和失败恢复。** Run: 上述测试；同机分别对普通目录、重名目标和只读目录检查，Windows 权限限制用其原生权限环境核实，不能只靠 Unix chmod 模拟。Expected: PASS，已有文件保持一致；清理失败明确 pending。
- [ ] **Step 5 — 提交命名文件。** Commit: `feat: publish validated outputs without overwriting files`。

### Task 5: 完整解码与计划一致性校验

**Files:** 创建 `src-tauri/src/media/validate.rs`、`src-tauri/tests/output_validation.rs`、`tests/fixtures/validation/{valid,wrong-frames,wrong-audio,shifted-start,decode-error}.json`；修改 media/mod.rs。

**Interfaces:** 消费 `MediaRunner`、`ProcessingPlan`、probe_input。产出 async `validate_output(runner:&dyn MediaRunner,plan:&ProcessingPlan,cancel:CancellationToken)->Result<ValidationResult,AppError>`。测试文件定义 async `validate_fixture(name:&str)->Result<ValidationResult,AppError>`，脚本化 runner 的成功退出与解码错误分开注入。

- [ ] **Step 1 — 写失败测试。** `zero_exit_with_decode_error_fails`、`video_frame_count_must_match`、`audio_presence_and_48k_must_match`、`duration_tolerances_follow_frame_rate`、`relative_av_start_is_preserved`、`changed_input_fails`。输出仅有容器头、缺音轨、错误像素格式也不能通过。

```rust
// output_validation.rs::zero_exit_with_decode_error_fails
assert_eq!(validate_fixture("decode-error.json").await.unwrap_err().code,
           ErrorCode::ValidationFailed);
// duration_tolerances_follow_frame_rate：30 fps 视频差 1/30 秒通过，超过则失败；音频差 50 ms 通过，超过则失败。
```

- [ ] **Step 2 — 运行失败测试。** Run: `cargo test --manifest-path src-tauri/Cargo.toml --test output_validation`。Expected: 校验逻辑未实现 FAIL。
- [ ] **Step 3 — 实现 validate_output。** 确认输入身份、大小、mtime 和 SHA-256 仍与计划匹配；临时输出非空，经 ffprobe 探测并用 FFmpeg `-v error -xerror -i ... -map 0:v:0 -map 0:a:0? -f null -` 完整解码。输出探测或解码失败归 validation_failed，工具缺失与取消仍保留各自错误码。复核 codec、尺寸、颜色标记中可验证的项目、帧率／帧数、yuv420p、音轨与时长。对相对音视频起点采用同一音频允差；未知源颜色给出缺失提示，不能冒称颜色一致。返回结果哈希与实测规格，校验不执行发布。
- [ ] **Step 4 — 运行测试并加入真实媒体对照。** Run: 上述测试；将报告的基线视频与原视频只读探测作方法核对，错误帧数、尾部截断和起点偏移样本必须失败。Expected: PASS；校验测试证明行为，不只比较 ffmpeg 参数文本。
- [ ] **Step 5 — 提交命名文件。** Commit: `feat: validate complete media outputs before publication`。

### Task 6: 单任务服务、取消和终态竞争

**Files:** 创建 `src-tauri/src/jobs/{mod,service,state,progress,log}.rs`、`src-tauri/tests/job_lifecycle.rs`、`tests/fixtures/progress/{chunked,malformed,late}.txt`；修改 contracts/job.rs、lib.rs 与 tests/support/fake_runner.rs。

**Interfaces:** 消费任务 1～5 的接口。产出 `JobService::new(runner:Arc<dyn MediaRunner>,record_dir:PathBuf,emit:SnapshotSink)->Self`；`SnapshotSink=Arc<dyn Fn(JobSnapshot)+Send+Sync>`。方法为 `inspect_input(path:String)->Result<MediaInfo,AppError>`、`cancel_inspection()->Result<(),AppError>`、`start_job(request:StartJobRequest)->Result<String,AppError>`、`snapshot(job_id:&str)->Result<JobSnapshot,AppError>`、`current_snapshot()->Result<Option<JobSnapshot>,AppError>`、`cancel_job(job_id:&str)->Result<JobSnapshot,AppError>`、`read_log(job_id:&str)->Result<LogExcerpt,AppError>`、`shutdown()->Result<(),AppError>`；除 new 外，以上方法均为 async。LogExcerpt 定义在 jobs/log.rs，含 text 与 truncated。检查输入和处理任务共享单一活动操作槽，不能同时运行。

- [ ] **Step 1 — 写失败测试。** `double_start_runs_one_child`、`cancel_during_validation_never_publishes`、`cancel_and_commit_choose_one_terminal_state`、`late_progress_cannot_reopen_terminal`、`cleanup_failure_is_pending`、`hung_probe_is_reaped_on_shutdown`。用测试 runner 的屏障控制关键交错，用 tokio 暂停时钟验证三秒超时，不靠 sleep 猜测竞态。

```rust
// job_lifecycle.rs::cancel_during_validation_never_publishes
assert_eq!(snapshot.state, JobState::Canceled);
assert!(!workspace.final_path.exists());
assert_eq!(runner.active_child_count(), 0); // 此处核实 fake runner；真实回收由任务 1 测试覆盖
```

- [ ] **Step 2 — 运行失败测试。** Run: `cargo test --manifest-path src-tauri/Cargo.toml --test job_lifecycle`。Expected: 编排／状态行为尚未实现 FAIL。
- [ ] **Step 3 — 实现服务和状态机。** 在状态锁下预留活动槽，再创建后台任务；所有失败路径释放槽。严格执行规格状态图与 version 单调增加；emit 在锁外。只解析完整的 -progress key/value 记录，按 frame／计划帧数显示运行进度，最大 0.99，避免非零 PTS 造成提前完成；验证与保存期间仍未完成，succeeded 才为 1.0。stderr 环形尾部最多 64 KiB；任务记录保存独立的版本、参数与结果。提交入口和取消入口互斥，不能发生 canceled 后又发布。关闭应用取消检查／任务，等待回收，再退出。
- [ ] **Step 4 — 运行生命周期和全后端检查。** Run: 上述测试、`cargo test --manifest-path src-tauri/Cargo.toml`、`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`。Expected: PASS；固定交错反复执行结果一致，进度结束不等同于任务成功。
- [ ] **Step 5 — 提交命名文件。** Commit: `feat: coordinate cancellable media processing jobs`。

### Task 7: Tauri 命令、处理界面和真实任务回归

**Files:** 创建 commands.rs、src/api/desktop.ts、src/features/processing/ 下三个产品文件及 ProcessingView.test.tsx、`src-tauri/tests/pipeline_real.rs`、`scripts/generate-fixtures.mjs`、`scripts/check-media.mjs`、`docs/development.md`；修改 App.tsx、lib.rs、tauri.conf.json、`src-tauri/capabilities/default.json`、package.json、锁文件。

**Interfaces:** 注册 probe_input、cancel_probe、list_presets、start_job、get_job_snapshot、get_current_job_snapshot、cancel_job、get_job_log、reveal_output；对应 JobService 与任务 3 方法，probe_input IPC 接收 path:String，get/cancel/log/reveal 接收 jobId:String，get_current_job_snapshot 无参数且返回可空快照。reveal_output 只定位该任务已经生成的路径。事件 `job_snapshot`，负载 JobSnapshot。前端 `DesktopApi` 给这些方法提供 Promise 返回类型，`subscribeSnapshots(listener)->Promise<() => void>` 返回解除订阅函数；useProcessing 只依赖这个接口，测试使用假 API。

- [ ] **Step 1 — 写失败行为测试。** `start_is_single_while_pending` 断言快速双击仅一次 startJob；`old_snapshot_never_overwrites_new` 断言 version 8 的 succeeded 不被 version 7 改成 running；`previous_job_events_do_not_replace_new_job`；`remount_has_one_subscription` 且通过 getCurrentJobSnapshot 恢复当前任务；`probe_cancel_resets_import_state`；`failed_validation_shows_error_without_result_action`。测试触发用户操作和 API 结果，不能只断言按钮存在。

```ts
// ProcessingView.test.tsx::old_snapshot_never_overwrites_new
expect(screen.getByText('处理完成')).toBeVisible();
expect(screen.queryByText('处理中')).not.toBeInTheDocument();
expect(api.startJob).toHaveBeenCalledTimes(1);
```

- [ ] **Step 2 — 运行失败测试。** Run: `npm run test -- --run src/features/processing/ProcessingView.test.tsx`。Expected: 新流程尚未实现 FAIL。
- [ ] **Step 3 — 接入命令和界面。** 左侧输入信息／预设／输出目录，右侧阶段进度／结果／日志；未知色彩信息显示“颜色信息未提供”。检查时可取消检查，任务时可取消任务；committing 阶段取消操作返回当前状态。注册最小文件对话框和结果定位权限，不给 shell 通用权限。订阅事件后立即读 getCurrentJobSnapshot；开始成功取得 jobId 后再次读该任务快照，以 jobId 和 version 合并，旧任务事件不能覆盖新任务。异步订阅完成时若组件已卸载，马上解除订阅。Rust 再次校验 request、jobId、路径和忙碌状态。
- [ ] **Step 4 — 运行真实回归与界面核实。** 定义 npm 的 `test:media` 调用 check-media.mjs；脚本准备生成的短 CFR、无音轨、不同 AV 起点、VFR、损坏文件，并运行 `cargo test --manifest-path src-tauri/Cargo.toml --test pipeline_real -- --ignored --test-threads=1`。定义 pipeline_real.rs 中的 `generated_media_end_to_end` 和 `provided_sample_end_to_end`；后者从脚本传入的 `VIDEO_EDITOR_SAMPLE_DIR` 只读取样本，检查 SHA、帧数、48 kHz、独立保存与取消。Run: `npm run test:media -- --sample-dir /Users/mailingfeng/projects/ai/video-editor/docs/requirements/20261002-research`、前端测试、typecheck/build；Expected: PASS。再在 `npm run tauri dev` 实际验证导入、取消、失败和定位结果；浏览器假 IPC 检查不能称作原生端到端验证。
- [ ] **Step 5 — 提交命名文件。** Commit: `feat: connect desktop controls to validated processing`。development.md 记录准备、测试和目前实际验证平台；原样本与生成媒体不提交。

### Task 8: 三个平台的安装包与发行验证

**Files:** 创建 `scripts/verify-release.mjs`、`scripts/verify-release.test.mjs`、`src-tauri/tauri.windows.conf.json`、`src-tauri/tauri.macos.conf.json`、`docs/release-verification.md`、`docs/third-party-notices.md`；修改 tauri.conf.json、工具锁清单和 development.md。

**Interfaces:** `verify-release.mjs --target <triple> --artifact <path>` 检查本机匹配的安装产物、工具版本与架构，并记录 JSON／文档证据；不把交叉编译视为已安装验证。发行记录包含 OS／CPU、构建版本、工具校验和、签名／公证、安装与任务结果。

- [ ] **Step 1 — 建立会失败的发行验收记录。** 三个目标初始为 NOT VERIFIED；wrong_architecture_is_rejected、missing_sidecar_is_rejected、missing_license_material_is_rejected 在脚本对应测试中断言非零退出，正常资源为零退出。本任务创建 `scripts/verify-release.test.mjs`，使用 Node 内建 test runner，不需要新增测试框架。
- [ ] **Step 2 — 运行失败检查。** Run: `node --test scripts/verify-release.test.mjs`。Expected: 验证器未实现 FAIL；不能用缺少其他平台主机的失败代替这组测试。
- [ ] **Step 3 — 实现打包和校验。** externalBin 使用固定 ffmpeg／ffprobe 资源；Windows NSIS 安装器准备 WebView2，提供离线安装模式；macOS 检查主应用与工具签名、架构、权限、公证。锁清单记录各目标真实资源和对应许可材料。使用已有发行凭据完成签名；没有凭据可生成明确标注的开发包，但发行验收保持 NOT VERIFIED，不购买服务或声称已公证。[Tauri 构建 CLI](https://v2.tauri.app/reference/cli/)
- [ ] **Step 4 — 在匹配的原生机器验证。** Run: Windows `npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis`；两种 Mac 分别 `npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg` 和 `npm run tauri -- build --target x86_64-apple-darwin --bundles app,dmg`。每台从安装产物启动，在无开发工具环境检查工具自检、转换、取消、中文／空格路径和重名输出。运行 verify-release 脚本及其测试；Expected: 三个平台各自有真实 PASS 证据。缺少平台、文件系统或签名环境时记录尚未验证的项，不将任务勾为完成。
- [ ] **Step 5 — 提交命名文件并做主线程最终复核。** Commit: `build: verify native desktop release packages`。运行全后端测试、前端测试、typecheck/build 和实际平台发行检查，核对规格验收表；只依据本次成功输出报告能力与限制。

## 每个任务的提交方式

先看 git status 和暂存清单，再只 stage 本任务命名文件；新增锁文件与 manifest 一并提交。提交信息写到仓库外的消息文件，使用 `git commit -F` 加同一显式文件清单，不把当前用户未跟踪文件加入。遵循 `compound-engineering:ce-commit`；任务标题中的 message 是主题，不要求绕过该技能的提交方式。

## 计划自审与验收映射

| 已批准规格 | 实现／验证归属 |
| --- | --- |
| 目标、证据边界、基础／研究预设区别 | Header、任务 3、development.md；额外样本滤镜是后续独立校准 |
| 入口、轨道、SDR、CFR、颜色与码率缺失 | 任务 2、3、5 |
| 探测、参数、执行、校验四模块 | 任务 1～6 |
| 单任务、不可变计划、版本快照、进度和日志 | 任务 3、6、7 |
| 临时目录、独立输出、输入变化、拒绝覆盖 | 任务 4、5、6 |
| 三秒取消、关闭、清理失败、异常退出恢复 | 任务 1、4、6、7 |
| 规格、全解码、帧数、音频及尾部允差 | 任务 5、7 |
| 中文／空格／引号路径、损坏、无音轨、忙碌 | 任务 2～7；引号文件名只在允许该字符的平台创建，其他平台验证明确路径错误 |
| 三平台工具资源、安装、签名、公证与许可 | 任务 1、8 |
| Review Focus 五项 | 分别由 2；3/5；4；1/6；6/7 的命名测试覆盖 |

所有步骤是执行阶段的未完成复选框，不代表已经运行。规划阶段仅核对覆盖、接口名称、链接和命令；没有安装依赖、脚手架或产品代码。

## 执行交接

用户先审核本实施计划。仓库要求在主线程顺序完成，推荐按此方式使用 `superpowers:executing-plans`；每个任务完成后验证并复核再进入下一个。计划确认只授权明确选择的执行方式，不自动授予发布到外部或付费服务的权限。

当前可用的是 macOS Intel 主机；Windows、Apple Silicon 与发行签名的检查需要相应环境。执行时可先完成可本地验证的开发任务，但缺少其他环境不能把“三平台发行验收”宣称为完成。
