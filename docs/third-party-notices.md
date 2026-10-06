# 第三方材料与发行状态

当前工具包仅供本地开发验证；发行材料尚未完整，发行验收状态保持 NOT VERIFIED。

## FFmpeg 与 ffprobe

当前 macOS Intel 资源为 evermeet 提供的 9.0.2-tessus，同一构建系列。下载来源、压缩包与可执行文件 SHA-256、版本、构建参数和工具输出的许可声明记录在 `tools/sidecars.lock.json`。

- 项目与下载入口：[FFmpeg](https://ffmpeg.org/download.html)。
- 构建提供者：[evermeet](https://evermeet.cx/ffmpeg/)。
- 该构建启用 `--enable-gpl`、`--enable-version3`、`--enable-libx264` 和多个外部库，工具声明采用 GPL v3 或后续版本。
- 工具自身的简短许可输出不等同于完整发行材料。需要对应构建的完整许可、FFmpeg 与所链接外部库的对应源代码和构建资料，并逐项记录校验和及包内位置。
- 所有材料准备后，将目标条目的 `distributionStatus` 改为 `ready` 并填写 `materials`；发行验证器核对包内实际文件，缺失材料会拒绝发行验收。

Windows x64 资源为 [Gyan 9.0.2 essentials 构建](https://github.com/GyanD/codexffmpeg/releases/tag/9.0.2)，由 [FFmpeg 下载页](https://ffmpeg.org/download.html)链接的构建提供者发布。版本与构建参数来自下载包 README 和 PE 内嵌字符串；它们不代表本机执行的 `-version`／`-buildconf`／`-L` 输出。原生 Windows 检查会另行采集这些输出。

下载包原样提供的 GPL v3 文本和 README 分别保存于 `tools/licenses/ffmpeg-windows-x64-GPLv3.txt`、`tools/licenses/ffmpeg-windows-x64-README.txt`，Windows 配置将它们打包到 `licenses/windows-x64/`；锁清单登记原包条目和 SHA。README 指向 [FFmpeg 源代码提交](https://github.com/FFmpeg/FFmpeg/commit/946fcce07b)，但这一链接和 GPL 文本不能代替所链接外部库的完整许可、对应源代码及构建资料。目标仍为 development-only，发行验收保持 NOT VERIFIED。

## 应用依赖

运行时使用 Tauri、其 dialog/opener 插件和 Rust 依赖，以及 React / React DOM。准确版本由 `Cargo.lock` 与 `package-lock.json` 固定；各依赖的完整许可证与声明需从锁定版本提取并随发行包保留。本文件列出材料要求，不能代替依赖逐项声明。

相关原始资料：[FFmpeg 许可页](https://ffmpeg.org/legal.html)、[Tauri 仓库](https://github.com/tauri-apps/tauri)、[React 仓库](https://github.com/facebook/react)。
