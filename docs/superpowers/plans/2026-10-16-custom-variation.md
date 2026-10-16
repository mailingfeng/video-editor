# 自定义变化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> 仓库适配：AGENTS.md 要求子代理任务在主线程顺序执行，优先采用 `superpowers:executing-plans`。用户已于 2026-10-16 确认本计划及主线程顺序实施方式。

**Goal:** 新增带 14 项独立强度档位、克制推荐布局、持久设置及严格变速校验的“自定义变化”模式。

**Architecture:** Rust 负责档位校验、任务内参数映射、设置原子保存及目标时间轴校验；批次只冻结公开档位并逐项生成参数。React 负责弹窗草稿和真实批次设置显示，沿用现有快照合并及任务生命周期。现有六套预设的媒体行为保持，新增模块按设置、画面、声音和时间计算分责。

**Tech Stack:** 现有 React 19 / TypeScript / Vite / Vitest，Rust 1.90.0 / Tokio / Serde / Tauri 2，锁定的 FFmpeg / FFprobe；不新增产品依赖。

**Spec:** [已确认规格](../specs/2026-10-16-custom-variation-design.md)，规格提交 `aca59bc`，用户已确认；实施基线 `1c712fd`，项目上下文 `46ddef9`。

## Global Constraints

- 每项 0～9 档，0 关闭该项；提供推荐档位与恢复推荐。
- 推荐前景缩放 2 档、背景模糊 2 档，均弱于“内容变化”；已有其他效果以 5 档对齐当前强度。
- 新增且内容变化没有的效果默认 0，包括裁切缩放、画面柔化和变速。
- 每任务随机加速或减速，不提供方向选择；首尾裁剪不在本次范围。
- 速度非零时 `D∈[5L,10L]`，`s=(1000±D)/1000`，使用约分有理数；保留全部视频帧及顺序。
- 变速默认保持音高；音高变化由独立的变调档位控制；裁切不改变最终输出宽高。
- `schemaVersion = 1`；设置文件 `custom-variation-settings.json`，最大 16 KiB；只保存档位。
- 同批视频共用档位，逐任务生成并固定参数；批次开始后不可修改设置。
- 前景缩放关闭时，背景模糊和前景动态位移不生效，保留其设置供再次开启。
- 仅允许最多一个 AAC 帧（1024 个采样）的末端取整补齐；不以长段补静音凑满目标时长。
- 保留现有媒体范围、双活动调度、输入保护、校验后排他提交、取消／关闭与参数加固边界。
- 内部 ID `custom-variation-v1`，版本 1；默认选中模式仍沿用现有“内容变化”；不扩大 IPC 文件权限。
- 实际参数不进入产品 UI、设置、用户报告／恢复 JSON；模板与私有脚本继续加固。
- 不提交测试视频、临时媒体或原有未跟踪参考图；不升级应用版本，不触发发布／推送流程。

## Review Focus

| 风险输入／条件 | 用户应得到的行为 | 必须落入的测试 |
| --- | --- | --- |
| 设置加载迟到、旧保存回包、弹窗编辑时外部批次已开始 | 不覆盖新选择或当前批次，草稿保留并正确锁定 | U6 `late_settings_do_not_override_active_batch`，U7 `external_start_locks_draft` |
| 保存与开始并发、第三个任务补位前配置被外部改写 | 全批档位只取开始时快照，不能出现半批新设置 | U5 `save_and_start_share_idle_boundary` / `refill_uses_frozen_levels` |
| 未来 schema、超大／损坏设置、符号链接、替换失败 | 明确回退或报错，不覆盖外部目标，不丢旧设置 | U2 `unsafe_or_future_settings_recover_without_rewrite` / `failed_replace_keeps_previous` |
| 极短声音、负或错开的音画起点、时间戳过大／不能精确表示 | 同步按统一变换处理；不能表示时明确失败，不补长静音或绕过校验 | U3 `reject_unrepresentable_timing`，U4/U8 尾部及错位音频测试 |
| 2×2 等小偶数画面、竖屏、非方形像素、高档裁切与位移叠加 | 合法尺寸、显示比例正确，位移不额外裁边；量化无效不能报虚假变化 | U4 `small_geometry_stays_valid`，U8 边框及 SAR 媒体测试 |

---

## 文件与接口分工

所有下列新符号均为拟新增接口。现有路径已核对；新增路径在所属任务创建。符号通过 Rust `contracts/mod.rs`、`media/mod.rs`、`presets/mod.rs` 或 `lib.rs` 正常导出，不建立通用效果框架。

| 文件 | 单一职责 |
| --- | --- |
| `src-tauri/src/contracts/custom.rs` | 14 项公开档位、推荐值、载入状态、模式匹配校验 |
| `src-tauri/src/contracts/timing.rs` | 内部 `TimingPolicy`，默认 Preserve |
| `src-tauri/src/settings/mod.rs`、`store.rs` | 有界、安全、原子的设置存储 |
| `src-tauri/src/media/timing.rs` | 目标起点／时长／精确 ticks 与预期时间轴扫描 |
| `src-tauri/src/presets/custom/mod.rs`、`video.rs`、`audio.rs` | 固定独立随机流、各效果映射及滤镜构建 |
| `src-tauri/src/jobs/service.rs`、`commands.rs`、`main.rs` | 设置互斥、批次冻结、公开 API 与应用目录接线 |
| `src/api/contracts.ts`、`desktop.ts` | 对应的 wire 类型及显式命令调用 |
| `src/features/processing/useCustomSettings.ts` | 保存值、草稿、异步身份及模式相关加载状态 |
| `src/features/processing/CustomSettingsDialog.tsx` | 五组设置与可访问的原生弹窗 |
| `src-tauri/tests/custom_settings.rs`、`custom_timing.rs`、`custom_plan.rs`、`custom_settings_store.rs` | 契约、时间、映射及磁盘行为测试 |
| `src-tauri/tests/custom_real.rs`、`tests/support/custom_media.rs` | 真实媒体、解码信号／布局比较、安装后工具选择 |

顺序依赖：U1 → U2 → U3 → U4 → U5 → U6 → U7 → U8。U2、U3 的逻辑虽可独立理解，仍按仓库要求顺序实施。每个任务先跑失败测试，再实现、验证和提交；提交只显式暂存本任务实际修改的文件。计划中的测试结果均是预期，尚未执行。

### Task 1: U1：公开档位契约与兼容字段

**Files:** Create `src-tauri/src/contracts/custom.rs`、`src-tauri/src/contracts/timing.rs`、`src-tauri/tests/custom_settings.rs`、`tests/fixtures/custom-settings.json`、`tests/fixtures/queue-custom-snapshot.json`；Modify `src-tauri/src/contracts/{mod,error,plan,batch}.rs`、`src/api/contracts.ts`、`src-tauri/tests/wire_contract.rs`。仅为新增字段适配 Rust 字面量：`src-tauri/src/{jobs/service,presets/basic}.rs` 与 `src-tauri/tests/{basic_plan,batch_lifecycle,batch_real,job_lifecycle,lifecycle_real,pipeline_real,user_reports}.rs`、`src-tauri/tests/support/media_fixture.rs`。

**Interfaces:**
- Produces `CustomVariationSettings`，含 schema_version 和规格中 14 个 `u8` 字段；`recommended() -> Self`、`validate(&self) -> Result<(), AppError>`。
- Produces `CustomSettingsStatus { settings: CustomVariationSettings, recommended: CustomVariationSettings, source: CustomSettingsSource, notice: Option<String> }`；source 为 `Default / Saved / Recovered`，wire 为 snake_case。
- Produces `ActiveProcessingSettings { preset_id: String, custom_settings: Option<CustomVariationSettings> }`；可选字段省略 None。
- Produces `validate_custom_selection(preset_id: &str, settings: Option<&CustomVariationSettings>) -> Result<(), AppError>`；`decode_custom_settings(Value)`、`decode_batch_settings(Value)`、`decode_start_request(Value)` 返回对应类型的 `Result<_, AppError>`，将设置解析错误映射为 InvalidSettings。
- Produces `TimingPolicy::Preserve` 与 `TimingPolicy::UniformSpeed { speed: Rational }`；ProcessingPlan 新增默认 Preserve 的 `timing`，ValidationPolicy 新增默认 None 的内部 `audio_target_samples: Option<u64>`；请求新增 `custom_settings: Option<_>`，QueueSnapshot 新增 `active_settings: Option<_>`。
- TS 定义 `CustomLevelKey = Exclude<keyof CustomVariationSettings, 'schemaVersion'>`，供 U6/U7 按字段编辑；Rust 与 TS 的公开类型只含档位，不包含 TimingPolicy 或音频采样校验目标。
- Test-only `zero_custom() -> CustomVariationSettings` 加入 `src-tauri/tests/support/media_fixture.rs`，合法 schema 为 1、14 档全部 0。

- [x] **1. 写失败测试**：`recommended_and_untrusted_settings`、`custom_mode_requires_settings`、`legacy_and_custom_wire_roundtrip`，固定值必须断言：

```rust
let settings = CustomVariationSettings::recommended();
assert_eq!((settings.foreground_scale, settings.background_blur), (2, 2));
assert_eq!((settings.crop_scale, settings.softness, settings.speed), (0, 0, 0));
assert_eq!((settings.pitch, settings.encoding, settings.dynamic_offset), (5, 5, 5));
assert!(validate_custom_selection("custom-variation-v1", None).is_err());
assert!(validate_custom_selection("content-variation-v1", Some(&settings)).is_err());
```

对每个字段分别提交 `-1 / 1.5 / 10 / null / "5" / 缺失`、未知字段／schema，断言 InvalidSettings；旧 fixture 序列化逐值不变，新 fixture 含公开档位且没有实际参数。
- [x] **2. 跑红**：`cargo test --locked --manifest-path src-tauri/Cargo.toml --test custom_settings --test wire_contract`；预期新符号未定义或新断言失败，不能由无关环境错误充当红灯。
- [x] **3. 实现**：按 Interfaces 增加 DTO、运行时验证和 `invalid_settings` 用户文案；请求／快照可选字段用 `serde(default, skip_serializing_if = "Option::is_none")`，未知字段拒绝。TS 保持相同字段和状态。旧 Rust 请求填 None、旧计划填 Preserve、旧 policy.audio_target_samples 填 None，不改变原六预设字符串。
- [x] **4. 跑绿**：上述测试、`cargo test --locked --manifest-path src-tauri/Cargo.toml`、`npm run typecheck`；新契约断言通过，原有非 ignored 回归无新增失败。
- [x] **5. 提交**：显式暂存 Files，消息 `feat: define validated custom variation settings (U1)`。

### Task 2: U2：有界设置保存与损坏回退

**Files:** Create `src-tauri/src/settings/{mod,store}.rs`、`src-tauri/tests/custom_settings_store.rs`；Modify `src-tauri/src/lib.rs`。

**Interfaces:** Consumes U1 DTO。Produces `SettingsStore::load(path: PathBuf) -> Self`、`SettingsStore::memory() -> Self`、`snapshot(&self) -> CustomSettingsStatus`、`save(&mut self, settings: CustomVariationSettings) -> Result<CustomSettingsStatus, AppError>`。memory 只供服务默认构造及测试，不写父目录；load 在初始化读取一次。

- [x] **1. 写失败测试**：`missing_file_uses_recommendation`、`save_survives_reload`、`unsafe_or_future_settings_recover_without_rewrite`、`failed_replace_keeps_previous`。读取超过 16384 字节、未知 schema、损坏 JSON／目录／链接得到 Recovered 且原字节不变；缺失为 Default 且 notice=None；保存后新 store 必须得到 Saved 和同一份档位。

```rust
let mut store = SettingsStore::load(path.clone());
assert_eq!(store.snapshot().source, CustomSettingsSource::Default);
let saved = store.save(CustomVariationSettings::recommended()).unwrap();
assert_eq!(SettingsStore::load(path).snapshot().settings, saved.settings);
```

失败替换用可控文件系统冲突／测试边界制造，断言旧内存值未更新、既有有效文件未被本操作截断、本操作临时文件清理；不能只在 root 下 chmod 后假定写失败。Unix symlink 与 Windows reparse／链接按平台能力分别验证，不能静默把未执行当通过。
- [x] **2. 跑红**：`cargo test --locked --manifest-path src-tauri/Cargo.toml --test custom_settings_store`，预期缺少存储模块。
- [x] **3. 实现**：只接受固定应用目录路径；读取前和持有文件句柄后检查普通文件／链接属性，限长读取至 16385 字节以识别超限。保存先验值，再写同目录 UUID 临时普通文件、sync、原子替换、成功后更新内存；拒绝链接／重解析目标。读取回退不自动覆写，错误提示采用规格文案。
- [x] **4. 跑绿**：重复上述测试和 `cargo test --locked --manifest-path src-tauri/Cargo.toml --test user_reports --test output_workspace`；磁盘中仅含 schemaVersion 与 14 档，不含种子、滤镜、输出路径。
- [x] **5. 提交**：消息 `feat: persist custom intensity settings safely (U2)`，只暂存本任务 Files。

### Task 3: U3：精确变速时间轴与验证分支

**Files:** Create `src-tauri/src/media/timing.rs`、`src-tauri/tests/custom_timing.rs`；Modify `src-tauri/src/media/{mod,timeline,probe,validate}.rs`、`src-tauri/tests/output_validation.rs`。

**Interfaces:** Consumes TimingPolicy。Produces `TargetTiming { video_time_base: Rational, video_start_pts: i64, video_duration_ticks: i64, frame_rate: Rational, audio_start_seconds: Option<f64>, audio_duration_seconds: Option<f64>, audio_samples_48k: Option<u64> }`；`target_timing(source: &MediaInfo, policy: &TimingPolicy) -> Result<TargetTiming, AppError>`；`project_video_pts(source: &VideoInfo, policy: &TimingPolicy, pts: i64) -> Result<i64, AppError>`。
- Produces `Timeline::with_timing(source: &VideoInfo, policy: &TimingPolicy) -> Result<Self, AppError>`，原 new 不变；`expected_timeline(runner: &dyn MediaRunner, source: &MediaInfo, policy: &TimingPolicy, cancel: CancellationToken) -> Result<Option<VideoTimeline>, AppError>`。Preserve 取原摘要；变速流式读源帧，不能对源哈希做“缩放”。
- Keeps `verify_specifications(plan, output)` 的原签名，作为 Preserve 兼容入口；UniformSpeed 调用该入口而未提供目标摘要时拒绝。新增 `verify_specifications_against(plan: &ProcessingPlan, output: &MediaInfo, expected: Option<&VideoTimeline>) -> Result<(), AppError>` 供完整校验和新测试使用；完整 validate_output 必须先取得真实目标摘要。

- [x] **1. 写失败测试**：`uniform_speed_projects_vfr_and_offsets`、`reject_unrepresentable_timing`、`wrong_middle_timestamp_fails`。源 `A/B=1/1000`、起点 5000 ticks、速度 103/100 时：

```rust
assert_eq!(target.video_time_base, Rational { num: 1, den: 103000 });
assert_eq!(project_video_pts(&source.video, &policy, 5000).unwrap(), 515000);
assert_eq!(project_video_pts(&source.video, &policy, 5033).unwrap(), 518300);
assert_eq!(project_video_pts(&source.video, &policy, 5080).unwrap(), 523000);
```

同样测试 97/100、非单位时间基分子、负音频起点；只改变中间帧但帧数／总时长相同必须失败。`short_audio_tail_is_not_hidden_by_mux_tolerance` 在 policy 目标 48000 samples、输出有效时长仅 47000 samples 时必须失败，即使差值小于 50 ms。timescale 超 i32、零／负速度、checked 乘法溢出或表达式整数超过 `2^53-1` 在媒体运行前拒绝。源 identity、原摘要不被目标值覆盖。
- [x] **2. 跑红**：`cargo test --locked --manifest-path src-tauri/Cargo.toml --test custom_timing --test output_validation`；确认失败指向新投影／校验分支。
- [x] **3. 实现**：按规格 §6.2 的统一锚点和有理数公式；UniformSpeed 输出 TB 为 `1/(B×P)`，相对 ticks 为 `Δpts×A×Q`。让现有 read_timeline 在 crate 内接受转换后的 Timeline，保留取消、错误、末帧策略；验证前后继续核对输入身份。规格检查用目标时长／起点／平均帧率及逐帧摘要，保留严格 Preserve 分支；audio_target_samples 为 Some 时，按输出有效轨道时长／尾部裁剪信息还原 48 kHz 有效采样数，误差至多一个采样，不能靠封装的 50 ms 容差通过；无可靠有效时长就失败。
- [x] **4. 跑绿**：上述测试及 `cargo test --locked --manifest-path src-tauri/Cargo.toml --test input_probe --test basic_plan --test job_lifecycle`；不得通过跳过目标摘要或扩大现有容差转绿。
- [x] **5. 提交**：消息 `feat: validate uniform-speed media timelines exactly (U3)`。

### Task 4: U4：冻结效果映射与自定义处理计划

**Files:** Create `src-tauri/src/presets/custom/{mod,video,audio}.rs`、`src-tauri/tests/custom_plan.rs`；Modify `src-tauri/src/presets/{mod,basic}.rs`、`src-tauri/tests/basic_plan.rs`。本任务只新增 build_plan 分支，暂不在 list_presets 启用未接好设置的 UI 入口。

**Interfaces:** Consumes U1 设置和 U3 时间函数。Produces `custom::ID`、`FrozenCustomParameters::resolve(settings: &CustomVariationSettings, job_id: &str) -> Result<Self, AppError>`、`timing(&self) -> TimingPolicy`、`video_filter(&self, video: &VideoInfo) -> Result<Option<String>, AppError>`、`audio_filter(&self, audio: &AudioInfo, video: &VideoInfo) -> Result<Option<String>, AppError>`、`encoding(&self, source: &MediaInfo) -> Result<Option<CustomEncoding>, AppError>`。`CustomEncoding { crf: u8, gop: u32, audio_bit_rate: Option<u64> }`；参数结构不派生序列化，数值无法安全表示时返回错误，不饱和截断 GOP。

- [ ] **1. 写失败测试**：`defaults_are_milder_than_content_layout`、`zero_has_no_extra_filters`、`each_level_is_finite_and_independent`、`small_geometry_stays_valid`、`speed_and_pitch_have_separate_time_controls`。逐项枚举 0..9，固定 jobId 的参数应可复现，改 hue 不改变 speed／pitch；推荐 2 档比例在偶数取整前为 0.92..0.944，模糊为参考的 0.4 倍。全 0 的 args／policy 除身份、元数据标识外对齐基础转码，timing=Preserve。

```rust
let mut settings = CustomVariationSettings::recommended();
settings.speed = 9; // 非零，避免两个原速值造成无意义的独立性断言
let a = FrozenCustomParameters::resolve(&settings, "fixed-job").unwrap();
let mut changed = settings.clone(); changed.hue = 9;
let b = FrozenCustomParameters::resolve(&changed, "fixed-job").unwrap();
assert_eq!(a.timing(), b.timing());
assert_eq!(a, FrozenCustomParameters::resolve(&settings, "fixed-job").unwrap());
```

数值测试放于 custom 模块私有测试可检查几何／sigma，不为测试向 IPC 暴露参数。添加 2×2、2×128、128×2、竖屏、SAR 处理及全 9；前景 0 不生成背景／位移，但保存档位不被改成 0。音频测试覆盖逐声道映射、speed/pitch 独立及未启用音频效果时无残留 limiter。
- [ ] **2. 跑红**：`cargo test --locked --manifest-path src-tauri/Cargo.toml --test custom_plan`；预期 custom 模块／新 preset 尚不可用。模块内几何／映射断言另外用 `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib presets::custom` 运行，不能遗漏私有单元测试。
- [ ] **3. 实现独立映射**：用 SHA256(`custom-variation-v1:jobId:effect-key`) 的独立流，落实规格 §3.1 全部公式、§3.2 抽样及固定效果顺序。video/audio 只构建本领域模板；模板通过 opaque hidden/render 编码。颜色／噪声／柔化作用主画面，裁切只发生于明确启用的 cropScale。裁切尺寸与位置按 4:2:0 对齐；裁切为 Cw×Ch 再缩放回 W×H 后，用 `setsar=sar*W*Ch/(H*Cw)` 恢复原 SAR，不把非方形像素强制变成 1:1。
- [ ] **4. 接入计划并验证尾部**：build_plan 校验模式／档位、生成一次 frozen 参数，将 timing 及目标策略写内存计划。UniformSpeed 使用目标编码器 TB／轨道 timescale，不能继续指定 demux TB；变调补偿和速度合并到逐声道处理，目标采样数来自 U3。自定义启用音频效果或 speed 非零时设置 policy.audio_target_samples，纯基础转码和旧预设保持 None。末端只允许有界 `apad=pad_len=1024` 后按目标采样数裁齐，不能用 `whole_len` 隐藏任意长度缺口；仍达不到目标时由独立采样数检查失败。最终解码声音及尾部标记由 U8 验证，不能仅断言滤镜含有 apad。
- [ ] **5. 跑绿并核对旧方案**：custom_plan、`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib presets::custom`、`cargo test --locked --manifest-path src-tauri/Cargo.toml --test basic_plan --test output_validation --test user_reports`；各选中目标应确实运行测试，原六预设 characterization 摘要不变。全 9 计划不能包含非法尺寸、NaN 或不受限参数。
- [ ] **6. 提交**：消息 `feat: build controlled custom video and audio variation (U4)`。

### Task 5: U5：设置命令与原生批次冻结

**Files:** Modify `src-tauri/src/{commands,main}.rs`、`src-tauri/src/jobs/service.rs`、`src-tauri/tests/{batch_lifecycle,job_lifecycle,user_reports}.rs`、`src-tauri/tests/support/controlled_runner.rs`；Create `src-tauri/tests/custom_service.rs`。

**Interfaces:** Consumes SettingsStore、DTO、build_plan。Produces `JobService::with_queue_sink_and_settings(runner: Arc<dyn MediaRunner>, record_dir: PathBuf, emit: SnapshotSink, emit_queue: QueueSink, settings: SettingsStore) -> Self`；旧构造委托 SettingsStore::memory，避免测试污染真实应用目录。
- Produces `JobService::custom_settings(&self) -> CustomSettingsStatus`（async）、`save_custom_settings(&self, settings: CustomVariationSettings) -> Result<CustomSettingsStatus, AppError>`（async），均使用现有 state Mutex，保存须 ensure_idle。
- Commands `get_custom_settings(state) -> Result<CustomSettingsStatus, AppError>`；`save_custom_settings(state, settings: serde_json::Value) -> Result<CustomSettingsStatus, AppError>`。start_batch／start_job 的外层请求参数以 Value 交给 U1 decode，然后传强类型服务；许可及 require_tools 顺序保持。

- [ ] **1. 写失败测试**：`invalid_batch_never_reserves_jobs`、`save_and_start_share_idle_boundary`、`refill_uses_frozen_levels`、`remount_snapshot_contains_actual_selection`。等待项／runner 调用计数在非法请求前后不变；运行时保存返回 Busy，磁盘值不变；第三项用开始时档位而非随后外部改写的磁盘文件。

```rust
let before = service.queue_snapshot().await;
assert!(service.start_batch(invalid_batch).await.is_err());
assert_eq!(service.queue_snapshot().await.version, before.version);
assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
```

竞争测试使用 barrier／现有 ControlledRunner 阻塞点，不靠 sleep；保存先取得锁则全批使用显式开始请求，开始先取得锁则保存 Busy。新增用户报告测试扫描 settings／history／recovery，确保仅 settings 可含档位、均无实际参数。
- [ ] **2. 跑红**：`cargo test --locked --manifest-path src-tauri/Cargo.toml --test custom_service --test batch_lifecycle --test user_reports`。
- [ ] **3. 实现**：Inner 持有 SettingsStore；main 从 app_data_dir 的固定文件载入。start_job 和 start_batch 均在锁内 validate_custom_selection 后预留，reserve_next 复制已冻结 custom_settings；queue_snapshot 仅在 batch 存在时生成 active_settings。保存的短小磁盘操作与 idle 检查在同一服务锁下完成；事件继续锁外发送，shutdown 不新增调度。
- [ ] **4. 跑绿**：上述目标及 `cargo test --locked --manifest-path src-tauri/Cargo.toml --test job_lifecycle`、`cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --features desktop -- -D warnings`；实际命令通过 generate_handler 注册，不增加 capability。
- [ ] **5. 提交**：消息 `feat: freeze custom settings across native batch jobs (U5)`。

### Task 6: U6：前端设置状态与快照恢复

**Files:** Create `src/features/processing/useCustomSettings.ts`、`src/features/processing/useCustomSettings.test.tsx`；Modify `src/api/desktop.ts`、`src/features/processing/{useProcessing,queueState}.ts`、`src/features/processing/useProcessing.test.tsx`、`src/features/processing/queueState.test.ts`、`src/features/processing/ProcessingView.test.tsx`。

**Interfaces:** DesktopApi 增加 `getCustomSettings(): Promise<CustomSettingsStatus>`、`saveCustomSettings(settings: CustomVariationSettings): Promise<CustomSettingsStatus>`。
- `useCustomSettings(api: DesktopApi, busy: boolean)` 返回 `status: CustomSettingsStatus|null, loading, saving, open, draft, error, openDialog(): void, setLevel(key: CustomLevelKey, value: number): void, restoreRecommended(): void, cancel(): void, save(): Promise<boolean>`；CustomLevelKey 为 14 字段名去掉 schemaVersion。
- useProcessing 返回上述状态为 `custom`，按 queue.activeSettings 恢复运行模式；start 只在自定义时携带保存值，并在 loading／open／saving 时拦截自定义开始，open／saving 时拦截所有开始。

- [ ] **1. 写失败测试**：`cancel_does_not_save`、`failed_save_keeps_draft`、`late_settings_do_not_override_active_batch`、`legacy_modes_do_not_wait_for_custom_loading`；队列版本 8 的 activeSettings 不被版本 2 回包替换，custom 原生拒绝时不改用户保存值。

```ts
expect(api.saveCustomSettings).not.toHaveBeenCalled(); // 修改草稿再取消
expect(result.current.draft?.foregroundScale).toBe(2); // 恢复推荐
expect(result.current.open).toBe(true); // 保存失败仍保留弹窗
expect(api.startBatch).not.toHaveBeenCalled(); // 草稿打开时重复开始
```

测试 fakeApi 新增接口及 deferred reject 支持，断言挂载代次变化、保存双击、卸载后回包及订阅清理；不把测试用假 API 扩成产品抽象。
- [ ] **2. 跑红**：`npm run test -- --run src/features/processing/useCustomSettings.test.tsx src/features/processing/useProcessing.test.tsx src/features/processing/queueState.test.ts`。
- [ ] **3. 实现**：保存值与草稿分开，保存使用 ref 锁与 request generation，成功前不更新 status；初始读返回若落后于保存或运行快照则不能覆盖。mergeQueue 保持按结构版本选择 activeSettings，job 事件仍只更新对应 job；旧快照没有 activeSettings 时保留“批次设置已冻结”的未知模式回退，不拿记忆值猜测。setLevel 拒绝非整数／越界值，不静默改成合法档位；只用原生返回的推荐值，不在前端复制参数映射表。
- [ ] **4. 跑绿**：上述测试、`npm run typecheck`、`npm run test -- --run`；原模式不因设置服务失败而不可用，已有乱序／重挂载回归通过。
- [ ] **5. 提交**：消息 `feat: manage custom settings drafts and batch restoration (U6)`。

### Task 7: U7：设置弹窗与第三模式入口

**Files:** Create `src/features/processing/CustomSettingsDialog.tsx`、`src/features/processing/CustomSettingsDialog.test.tsx`；Modify `src/features/processing/{ProcessingView.tsx,processing.css,ProcessingView.test.tsx}`、`src-tauri/src/presets/basic.rs`、`src-tauri/tests/basic_plan.rs`。

**Interfaces:** `CustomSettingsDialog({draft, recommended, busy, saving, error, onLevelChange, onRestore, onCancel, onSave})`：draft/recommended 为 U1 CustomVariationSettings，busy/saving 为 boolean，error 为 string|null，onLevelChange 为 `(key: CustomLevelKey, value: number) => void`，onRestore/onCancel 为 `() => void`，onSave 为 `() => Promise<boolean>`。回调绑定 U6，不直接 invoke；开启／关闭由父组件挂载和原生 dialog 生命周期处理。

- [ ] **1. 写失败测试**：`only_custom_exposes_settings`、`all_fourteen_levels_are_accessible`、`dependencies_preserve_hidden_values`、`external_start_locks_draft`。默认仍 content-variation-v1；只有自定义有设置入口；14 个整数滑杆名称明确；前景 0 禁用背景／位移但重新开启恢复旧值；保存中不能关闭，外部开始后能取消但不能改／存；恢复推荐不立即写盘。

```ts
expect(screen.getByRole('slider', {name:'前景缩放'})).toHaveValue('2');
expect(screen.getByRole('slider', {name:'背景模糊'})).toHaveValue('2');
expect(screen.getByRole('slider', {name:'变速'})).toHaveValue('0');
expect(screen.getAllByRole('slider')).toHaveLength(14);
```

复用当前 jsdom dialog shim，仅以其测试行为；焦点限制和返回须真浏览器验证。非法 API 错误不直接作为任意 HTML／原始工具消息显示。
- [ ] **2. 跑红**：`npm run test -- --run src/features/processing/CustomSettingsDialog.test.tsx src/features/processing/ProcessingView.test.tsx`。
- [ ] **3. 实现**：五组固定顺序、最大宽 880px、可滚动内容及可见操作区；每项仅档位、推荐标记和简短说明，裁切提示可能影响边缘字幕。不显示实际倍速／频率／比例。接入 loading、recovered、保存失败及运行中提示。
- [ ] **4. 启用入口并跑绿**：list_presets 在原两项后加 custom-variation-v1 / 1 / 自定义变化；更新“仅两模式”旧断言为三模式但保持原两项身份与默认规则。运行上述测试、`npm run typecheck`、`npm run build`、`cargo test --locked --manifest-path src-tauri/Cargo.toml --test basic_plan`。
- [ ] **5. 提交**：消息 `feat: expose custom variation intensity settings (U7)`。

### Task 8: U8：真实媒体、原生门禁与知识交付

**Files:** Create `src-tauri/tests/custom_real.rs`、`src-tauri/tests/support/custom_media.rs`；Modify `scripts/{generate-fixtures,check-media}.mjs`、`.github/workflows/{windows-x64,macos-intel}.yml`、`src-tauri/tests/{batch_real,user_reports}.rs`、`scripts/verify-release.test.mjs`（必要时修改 verify-release.mjs 的自定义模板特征）、`PROJECT.md`、`ARCHITECTURE.md`、`DESIGN.md`、`docs/{agent-workflow,windows-desktop-acceptance,macos-intel-desktop-acceptance,README}.md`。验证后 Create `docs/evidence/custom-variation-YYYYMMDD.json`，使用实际完成日期，不预先写成功记录。

**Interfaces:** `support/custom_media.rs` 提供 `tool_paths() -> ToolPaths`，优先 VIDEO_EDITOR_INSTALLED_EXE；异步测试函数 `decode_pcm(path: &Path) -> DecodedPcm`、`decode_frames(path: &Path) -> Vec<u8>`、`run_custom(input: &Path, settings: CustomVariationSettings, job_id: &str) -> Result<ValidatedCustomOutput, AppError>`。DecodedPcm 含 `sample_rate: u32, channels: u8, samples: Vec<f32>`（交错声道），ValidatedCustomOutput 含 `result: ValidationResult, plan: ProcessingPlan, directory: tempfile::TempDir`。最后一项使用真实 NativeRunner、prepare_workspace、build_plan 和 validate_output，临时目录由返回对象持有，不能绕过归属／私有脚本；测试内 plan 不写盘。服务级批次测试继续从 JobService 运行，验证真实调度与取消。

- [ ] **1. 写失败媒体测试并准备 fixture**：新 generated_ 用例标 ignored，但真实运行选中。添加横屏／竖屏边框标记、2×2、非方形像素、CFR／VFR、已知逐声道音调及最后 100 ms 非静音标记；复用已有 offset、全范围、无音轨及 surround fixture。
  - `generated_custom_speed_preserves_frames_and_pitch`：显式固定 jobId 覆盖正／负速度，VFR 摘要与目标精确相等；仅 speed 的音调误差 ≤2%，帧数不变。
  - `generated_custom_pitch_surround_and_tail`：1／2／5.1／7.1 声道的目标频率相对误差 ≤2%；LFE 采用 90 Hz 原信号；尾标记在目标末尾可检测，末尾合成补齐 ≤1024 samples，波形有限且无过载。
  - `generated_custom_layout_and_crop`：隔离编码和其他效果，对照源图／内容变化／custom 推荐／custom 布局 5；确认推荐前景更大、背景更弱，crop=0 四边标记保留，crop>0 只去计划区域，SAR 未意外变形。
  - `generated_custom_all_levels_and_boundaries`：全部 0、推荐、14 项单独 9、全 9；坏输入失败、其余有效输入通过完整校验；不强求静音、低分辨率相邻档位产生不同哈希。
  - batch_real 增加自定义设置的两活动＋一等待、活动取消／关闭、单项失败继续；保留输入 SHA 与输出排他检查。
- [ ] **2. 运行选中的媒体红灯**：先 `npm run prepare:sidecars`，再 `node scripts/generate-fixtures.mjs`；设置 VIDEO_EDITOR_FIXTURE_DIR 后 `cargo test --locked --manifest-path src-tauri/Cargo.toml --test custom_real generated_ -- --ignored --test-threads=1`。应有针对真实缺陷的失败；不能让“0 tests”或缺工具错误充当红灯。若现实现已通过，用故意错误目标／被截断的对照输出验证断言能拒绝，不无意义回退正确代码。
- [ ] **3. 修复实际差距并接入门禁**：只修证据指向的代码，必要时回到对应任务单元；保持公式与验收边界。check-media 顺序追加 custom_real，非零退出即失败；Windows 安装后单独运行 custom_real，显式设置 fixture／installed exe 和检查 LASTEXITCODE；macOS 已安装步骤通过 check-media 覆盖该目标。fixture 在忽略目录，不依赖研究原片才能覆盖新功能。
- [ ] **4. 运行本地自动门禁**：按下方完整命令组执行；新 ignored 用例确实运行，旧六预设摘要及既有媒体回归通过；加固测试证明新模板没有因普通格式字符串绕过编码，设置／历史不落实际参数。失败保留真实日志，不写验证已完成。
- [ ] **5. 桌面与原生验收**：按规格 A16 验证三种宽度、键盘、焦点、保存失败／重启恢复；有条件的平台原生构建、安装资源与安装后 custom_real 另记录。不能因本地支持一个平台就声称两个平台完成。执行本计划本身不授权推送、远程发布或创建 Release；未执行项写入交付限制。
- [ ] **6. 同步与提交**：更新四份当前上下文、原生验收步骤与 docs 索引；证据关联实际源码提交、schema／映射版本、测试命令、样本范围、截图／日志及未执行项。消息 `test: verify custom variation media and desktop behavior (U8)`，只暂存代码、测试和文档，不含媒体或参考图。

## 完整本地门禁与交付判定

执行前按 using-git-worktrees 在执行阶段建立隔离工作区，带入已确认规格和本计划；保留本目录的未跟踪参考素材。不要把规划阶段的文档改动或本机缓存状态误当测试基线。

```sh
npm run test -- --run
npm run typecheck
npm run build
npm run test:sidecars
npm run test:release
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --features desktop -- -D warnings
npm run test:media
git diff --check
```

若本地有研究原片，再执行 `npm run test:media -- --sample-dir docs/requirements/20261002-research`；这是补充，不代替无需私有样本的 generated_ 门禁。发布／签名／公证与平台审核不属于本计划完成声明。

| 规格验收 | 负责单元 |
| --- | --- |
| A1 | U7、U8 |
| A2 | U1、U4、U7 |
| A3 | U2、U5、U6、U7 |
| A4 | U2、U6 |
| A5 | U1、U5、U6 |
| A6 | U4、U5、U6 |
| A7–A9 | U4、U8 |
| A10–A12 | U3、U4、U8 |
| A13–A15 | U4、U5、U8 |
| A16 | U7、U8 |

最终以实际验证结果区分“已实现／本地自动检查通过／已验平台／未验范围”。检查未跟踪文件、分支差异与文档链接，按仓库规则在主线程做全分支审查；没有独立审查就如实说明。用户确认本计划与执行方式后才进入上述任务。
