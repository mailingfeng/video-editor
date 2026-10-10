# 帧序桌面开发

应用支持文件与文件夹批量导入 MP4、MOV、M4V、MKV 和 WebM，输出为 H.264 / MP4。输入目前要求一个主视频轨、零或一个音轨、偶数尺寸的 8 bit SDR 视频；音频支持 1–8 声道，包括 5.1 和 7.1。

## 启动

安装 Node.js 24、npm 11、Rust 1.90，以及 [Tauri 的系统依赖](https://v2.tauri.app/start/prerequisites/)。仓库的 `rust-toolchain.toml` 固定 Rust 工具链。

```sh
npm ci
npm run prepare:sidecars
npm run tauri dev
```

媒体工具只从 `tools/sidecars.lock.json` 中已校验的来源准备，不调用系统 PATH 中的 FFmpeg。当前锁定 macOS Intel 的 9.0.2-tessus 和 Windows x64 的 Gyan 9.0.2 essentials 构建；Windows 工具已核对下载包／二进制 SHA 与 PE x64 架构，尚未原生执行。Apple Silicon 仍缺少锁定资源，准备脚本会明确拒绝缺失目标。Windows 环境、构建与安装检查见 [Windows x64 开发包准备](windows-x64.md)。

本机 npm 共用缓存存在权限问题，可使用 `npm ci --cache .superpowers/npm-cache`。不要改动全局缓存权限。浏览器 `npm run dev` 可查看界面，本地文件操作需要桌面应用。

## 验证

```sh
npm run test -- --run
npm run test:release
npm run test:sidecars
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features desktop -- -D warnings
npm run test:media
npm run test:media -- --sample-dir /path/to/research-or-samples
# 补充原样本探测与原生校验回归（填写真正含有原视频.mp4 的目录）
VIDEO_EDITOR_SAMPLE_DIR=/path/to/research-or-samples cargo test --manifest-path src-tauri/Cargo.toml --test input_probe --test output_validation -- --ignored --test-threads=1
```

最后一条命令在指定目录及其父目录查找 `原视频.mp4`，仅以只读方式探测并转换，结果写入测试临时目录。会验证 2713 帧、48 kHz 音频、独立输出、转换与取消前后的输入 SHA。生成媒体保存在忽略的 `.media-fixtures/`，样本与产物不提交。

生成回归覆盖 CFR / VFR、上述五类容器、5.1 / 7.1 音频、有无音轨、不同音画起点、全范围 YUV、HDR、RGB 和损坏容器。VFR 保留帧数与逐帧时间戳，转码与完整解码校验均使用 `-fps_mode:v passthrough -enc_time_base:v demux`；输出逐帧时间戳用归一化有理数秒计算 SHA256 后比对。帧率由实际帧数和时长计算；帧时长缺失时使用相邻时间戳，最后一帧只能采用帧时长、轨道终点或已确认的均匀帧间隔。音频轨道时长缺失时追加读包探测，已知声道布局保持一致，多声道码率按声道数调整。

HDR、10 bit、RGB 色彩矩阵转换、奇数尺寸、多主轨仍需另行设计转换策略。全范围 H.264 被 ffprobe 报告为 `yuvj420p` 时按等价的 8 位 4:2:0 检查，并核对已知范围；已知的范围通过 `h264_metadata` 写入码流，其他未知颜色标记保持未知。前端测试覆盖重复开始、乱序快照、跨任务旧事件、重挂载与订阅清理、取消检查、校验失败、部分色彩信息和新输入与历史结果的区分。

对本地兼容性样本进行只读验收（测试结果写入临时目录，源视频不会被修改）：

```sh
VIDEO_EDITOR_COMPAT_SAMPLE="$PWD/docs/requirements/20261002-research/h264.mp4" \
  cargo test --locked --manifest-path src-tauri/Cargo.toml --test pipeline_real \
  provided_compatible_media_end_to_end -- --ignored
```

## 已执行的桌面操作

2026-10-02，macOS Intel，使用 `tauri dev` 的真实窗口与系统文件对话框完成：

- 导入 `av-offset.mp4`，显示 36 帧、44.1 kHz 单声道、颜色信息未提供。
- 选择独立输出目录，执行转换，完成全量校验后显示 100% 与新文件路径。
- 点击“在文件夹中显示”，Finder 选中了该任务生成的 MP4。
- 对原样本取消检查，输入恢复到未选择状态；再次导入，在转换阶段取消任务，显示“已取消”。
- 导入损坏 MP4，显示“媒体无法完整读取”，开始按钮不可用。
- 关闭窗口，Tauri 开发进程正常退出。

浏览器另检查了顶部导航与日志页、1280 像素和 390 像素宽度的布局。浏览器检查不替代桌面端验证。Windows / Apple Silicon 的实机操作尚未执行。

同日从开发 DMG 复制安装应用，关闭开发服务器后，实测中文／空格路径转换、原样本 2713 帧转换、转换阶段取消、结果定位、重名拒绝与已结束进程的异常残留启动清理。资源检查与实测证据见 [发行验证记录](release-verification.md)。开发包没有签名／公证，完整许可和对应源代码材料也未齐，不能作为已验收发行版。

## 本地记录

应用数据目录的 `job-records/` 保存工作目录归属记录、冻结参数、最新终态、有限长度日志与校验结果。启动时只尝试清理应用登记且归属校验通过的临时目录；不递归删除用户输出目录。退出前取消可取消的任务并等待子进程退出，保存提交阶段等待完成。

基础转换不包含样本中的额外亮度、色彩或音频调整，也没有平台重复检测的效果承诺。上传验收需由持有平台账号的用户另行执行。

界面还可选择“样本处理（实验）”预设，近似 `原视频-1.mp4` / `编辑后视频-1.mp4` 的亮度、编码结构与音频频谱变化。该预设仍保留 VFR 时间轴与 1–8 声道；更适合人声素材。用户随后确认上次提供的实验预设测试视频未通过审核，作品因高度相似被限制为仅主页可见。校准参数、对照指标、未恢复的处理与复测反馈见 [样本 1 校准记录](requirements/20261002-research/sample-1-calibration.md)。本地媒体指标不能证明平台原创审核效果。

用户确认参考输出仍可通过，并补充第二组样本。两组的音频相位、约 5 毫秒延迟、第二组额外高频成分及编码配置差异见 [两组差异分析](requirements/20261002-research/reference-pairs-analysis.md)。该分析不等于生产预设已更新或平台效果已修复。

随后新增“样本处理（相位实验）”v2 预设：保持 v1 画面和编码策略，使用因果高通 / 低通、约 5 毫秒全声道延迟与固定增益。两份实际应用 MP4 的 AAC 波形与参考的零时差相关为 0.8556 / 0.8330，源时间轴和 v1 画面均保留；用户现已反馈两份上传均未通过平台审核。冻结参数、完整输出身份和测试结果见 [v2 验证记录](requirements/20261002-research/phase-v2-validation.md)。`npm run test:media` 同时回归 v1 / v2，并包含 7.1 非静音信号的逐声道相位 / 增益检查。下一轮分别验证编码、动态增益与高频信号，见 [第三轮实验](requirements/20261002-research/round3-experiments.md)。

第三轮提供八份独立对照后，用户希望减少上传数量并叠加处理，先复测两份 [C 组合版](requirements/20261002-research/round3-combined-validation.md)。组合版从原片一次编码，叠加编码配置、动态增益、适配的 AAC 带宽和额外高频信号，完整媒体校验通过。2026-10-10 用户反馈第二组通过原创声明审核，第一组仍因高度相似仅主页可见；尚未加入应用预设，不能视为通用审核效果已验证。第一组 [移除高频信号的第四轮对照](requirements/20261002-research/round4-carrier-ablation.md) 随后也被反馈未通过原创审核。第二组通过样本的参数与文件保持冻结；完整结果见 [审核结果整理](requirements/20261002-research/platform-review-matrix.md)。

用户随后明确任意输入、同一原片每次独立处理后都应通过原创审核，且“必须保持内容基本不变”，详见 [更新的需求](requirements/20261002-research/requirement.md)。固定 C1 参数仅换输出路径重跑得到相同文件哈希，当前链没有按次变化，见 [重复运行检查](requirements/20261002-research/repeat-processing-audit.md)。既有第一组参考现也被反馈未通过；重新上传该既有文件不能验证参考软件的多次处理能力。下一步需要参考软件对同一原片多次独立处理的新输出及每次审核结果。文件或信号不同不等于平台判定原创；当前不能承诺普遍或每次通过。

后续新增可选“重复处理（实验）”预设 `repeat-variant-v1`，以冻结的任务种子加入轻微画面扰动，保持镜头顺序和基础音频策略。两份原片各导出 A / B：解码画面与 H.264 码流均不同，AAC 码流与各自基础转码一致；完整时间轴校验和相对基础转码的画质预算均通过。VFR、多声道、全范围、无音轨和 WebM 回归通过；当轮 Mac Intel `.app` 通过开发资源检查，版本仍为 `0.0.1-beta.3`。用户随后确认这四份输出均未通过原创审核，提示仍为高度相似。参数、全片指标和反馈见 [重复处理本地验收](requirements/20261002-research/repeat-variation-validation.md)。

随后新增可选“组合变化（实验）”预设 `repeat-combined-v2`，按任务冻结小角度几何变化、色调、锐化、更强的时间噪声、音频频谱与相位、动态增益和编码参数。原尺寸、帧数、完整时间轴及声道布局保持，允许轻微边框、色调与音色变化。两份原片各输出一份，解码画面和 PCM 均与基础转码不同，全片 MAE 为 7.0833 / 9.6656，均满足当轮预算 ≤ 12。当轮 70 项 Rust、29 项前端、13 项生成真实媒体测试及 1 项 `h264.mp4` 五预设实样回归通过。用户随后确认两份均未通过，仍提示高度相似。完整证据、冻结应用身份和视频见 [组合变化验收](requirements/20261002-research/combined-variation-validation.md)。

用户现允许声音变调和明显画面变化，因此再新增独立可选“内容变化（实验）”预设 `content-variation-v1`：逐声道升高或降低约 2–3 个半音并补偿时长，完整原图缩小为动态前景，配合同源模糊背景和色彩变化。原尺寸、帧数、逐帧时间戳、音画起点及声道布局继续校验，静音或无音轨不承诺声音差异。71 项 Rust、30 项前端、14 项生成真实媒体测试及 1 项 `h264.mp4` 六预设实样回归通过；Mac Intel `.app` 重新构建并通过开发资源检查，仍为 `0.0.1-beta.3`，没有新 Release。Windows 原生构建、实际桌面交互和新输出的平台审核未在本轮执行。两份完整输出的记录见 [内容变化验收](requirements/20261002-research/content-variation-validation.md)，本地视听变化不能证明平台认定原创。

2026-10-10 用户反馈最新“内容变化”交付批次通过平台审核；未逐份指定通过文件，未独立核验上传哈希或审核类别。记录为本批次用户反馈，不扩大为任意视频或多次处理必过的结论。

`0.0.1-beta.4` 加入截止北京时间 2026-12-31 的使用许可：原生层比较本机与两台 NTP 服务器有效时间，采用较晚值，并在本次运行中以单调时钟推进。启动单个任务和批次均检查许可；界面到期禁用按钮并提供提示，现有批次继续完成。网络查询有界超时、每分钟刷新，失败回退本机及本次运行已确认的时间；详见 [许可说明](license.md)。
