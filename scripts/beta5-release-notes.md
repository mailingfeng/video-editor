帧序 `0.0.1-beta.5` 内测版，提供 Windows x64 与 macOS Intel 安装包。

### 本次更新

- 处理选项简化为“内容变化”和“轻微变化”，默认选择“内容变化”；移除实验字样、具体处理参数及技术说明。
- “日志与报告”改为“日志报告”，移除顶部右侧提示及底栏。
- 日志时间包含日期，采用北京时间，记录阶段、进度、结果及有帮助的异常提示。任务详情与日志不再显示工具命令、参数或原始工具输出。
- 旧任务日志按已保存的开始、结束时间及结果恢复摘要；已有完整日期的日志原样保留。
- NTP 服务器扩展为六台，访问失败时静默使用本机与本次运行已确认的较晚时间。许可仍有效至北京时间 **2026-12-31 当天结束**。
- 保留批量处理、独立保存、可变帧率和多声道支持。

### 下载与运行

- Windows x64：`frameshift_0.0.1-beta.5_windows_x64_setup.exe`。
- macOS Intel：`frameshift_0.0.1-beta.5_macos_intel.dmg`。
- 使用附件 `SHA256SUMS` 核对下载内容；许可规则见 `license.md`，桌面验收步骤见平台说明。

### 验证记录

- [Windows x64 原生构建、安装及媒体回归](https://github.com/mailingfeng/video-editor/actions/runs/38147431454)。
- [macOS Intel 原生构建、DMG 安装及媒体回归](https://github.com/mailingfeng/video-editor/actions/runs/38147433545)。
- 两个平台分别通过 87 项后端测试、35 项界面测试、17 项发布工具测试、14 项生成媒体回归及 14 项安装后媒体回归。
- 附件包含平台构建记录、安装资源记录、验证日志、桌面验收说明及第三方材料说明；上传后的附件大小与 SHA-256 均已核对。

沿用未签名内测通道；Windows Authenticode、Apple Developer ID 签名、macOS 公证及本版本完整人工安装后桌面验收尚未完成，正式发行状态为 `NOT VERIFIED`。第三方材料状态见附件说明。

源提交：`c56779a0a9f77bc8b8ecf79ab1d1c20233c8b701`；标签：`v0.0.1-beta.5`。
