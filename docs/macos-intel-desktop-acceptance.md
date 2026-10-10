# macOS Intel 内测包验收

`0.0.1-beta.4` 使用许可至北京时间 2026-12-31 当天结束。启动时确认许可查询完成后可开始处理；断网时应说明使用本机时间。到期后“开始处理”禁用，悬停或键盘聚焦显示更新许可提示，已保存结果和日志仍可查看。截止时刻、较晚时间和回调时钟已由自动测试覆盖，人工桌面验收仍须独立记录；详见 [许可说明](license.md)。

适用目标：`x86_64-apple-darwin`，Intel Mac，macOS 12 或更高版本。DMG 中包含“帧序.app”，将其拖到 Applications 后运行。FFmpeg 与 FFprobe 位于应用的 `Contents/MacOS/`，无需安装 Homebrew、Node.js、Rust 或系统 FFmpeg。

当前内测包未使用 Apple Developer ID 签名和公证。首次打开可能受到 Gatekeeper 拦截；按系统提示到“系统设置 → 隐私与安全性”确认打开，仅对确认来源的本次内测包操作。正式签名、公证与完整发行验收仍待补齐；Actions 通过仅表示构建、安装资源和包内工具的视频回归检查通过。

1. 核对 Release 的 SHA256SUMS 与 DMG 文件，记录源提交、系统版本与 CPU 型号。
2. 安装后运行，选择带中文和空格路径的 MP4 和文件夹。文件夹仅导入当前层 MP4，重复路径不重复，同名不同路径可区分。
3. 选择独立输出目录并开始处理，最多两个文件同时活动，每行显示自身阶段与进度，正式保存前不显示 100%。损坏文件独立失败，后续项继续。
4. 分别打开两行的“视频信息”和“查看日志”，确认源信息、路径、错误和日志归属正确，日志可刷新，关闭或按 Esc 后焦点回到对应按钮。
5. 取消等待项与活动项，确认不影响其他文件。活动项完成清理后才补位，关闭窗口时所有自有媒体进程结束，未启动项不再开始。
6. 对成功项点击“显示结果”，确认定位该文件的输出；核对源文件 SHA256 保持不变。单视频选择和拖入仍可完成处理。
7. 在没有开发工具和系统 FFmpeg 的机器上执行以上流程，并保留截图、日志、输出路径与工具版本。此项需要人工记录，CI 不能代替。

可直接使用包内工具核对版本：

```sh
"/Applications/帧序.app/Contents/MacOS/ffmpeg" -version
"/Applications/帧序.app/Contents/MacOS/ffprobe" -version
```

相关系统说明：[Apple 打开来自未知开发者的 Mac 应用](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac)。
