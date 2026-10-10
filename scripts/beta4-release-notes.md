帧序 `0.0.1-beta.4` 内测版，提供 Windows x64 与 macOS Intel 安装包。

### 本次更新

- 新增可选“内容变化（实验）”：逐声道声音变调、时长补偿、完整原画面动态前景与同源模糊背景，按任务冻结不同参数。保留尺寸、帧数、完整时间轴、音画起点和 1–8 声道布局。
- 包含样本相位、轻微重复变化及组合变化等实验预设。最新内容变化交付批次已收到用户“通过平台审核”的反馈，具体文件与审核类别未逐份确认。
- 使用许可有效至北京时间 **2026-12-31 当天结束**，自 2027-01-01 00:00 起禁止开始新的处理任务。
- 比较本机及 `ntp1.aliyun.com`、`ntp1.ntsc.ac.cn` 的有效时间，采用较晚值；并行查询、2 秒超时、每分钟刷新。网络不可用时采用本机及本次运行已确认的较晚时间，并显示提示。
- 到期后禁用“开始处理”，悬停或键盘聚焦显示更新许可提示；后端单个任务与批次启动也检查许可。已启动的批次继续完成，结果、日志与取消操作仍可使用。
- 保留 VFR、多容器、多声道、批量处理、独立保存、重名保护和退出恢复。

### 下载与运行

- Windows x64：`frameshift_0.0.1-beta.4_windows_x64_setup.exe`。
- macOS Intel：`frameshift_0.0.1-beta.4_macos_intel.dmg`。
- 两个平台均附带 FFmpeg 和 ffprobe；Windows 按需联网准备 WebView2。
- 使用附件 `SHA256SUMS` 核对下载内容；许可规则详见 `license.md`，桌面验收步骤见平台说明。

### 验证记录

- [Windows x64 原生构建、安装及媒体回归](https://github.com/mailingfeng/video-editor/actions/runs/38056052330)。
- [macOS Intel 原生构建、DMG 安装及媒体回归](https://github.com/mailingfeng/video-editor/actions/runs/38056086207)。
- 验证结果、安装资源与校验和详见发布附件；本地 NTP 查询本轮超时，实际验证了离线回退。截止时刻、取较晚时间、回调时钟和 UDP 请求/超时由自动测试覆盖。

沿用未签名内测通道；Windows Authenticode、Apple Developer ID 签名、macOS 公证及本版本完整人工桌面验收尚未完成，正式发行状态为 `NOT VERIFIED`。普通 NTP 与本机时钟不提供防篡改保证，本次运行的时间记录不跨重启保存。第三方材料状态见附件说明。

源提交：`23c18f01057f82c0f70a978c8cef97a9aeb925f4`；标签：`v0.0.1-beta.4`。
