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
