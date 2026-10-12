# Windows 安装模板

`installer.nsi` 基于 [Tauri CLI 2.12.1 的 NSIS 模板](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.1/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi)。原始 Git blob 为 `480d30c38bf7b5acc280619aba07d14f9a1e8a0e`，原始文件 SHA-256 为 `dabed59013b1d78b879a1a85bc7f2eed2993b33a9a90cdabe5946de3d3950597`。按随附 MIT 许可证使用，许可证同时随 Windows 安装包分发。

本项目发布当前用户 NSIS 安装包。升级和同版本重装直接覆盖原安装目录，不显示维护选项，不执行旧卸载器；旧卸载器或版本字段丢失仍可修复。安装目录优先取已有厂商记录，缺失时读取卸载记录的 `InstallLocation`。已有安装时沿用该目录，不通过 `/D` 迁移位置。新安装仍可选择目录。

版本降级检查放在安装阶段，覆盖图形、静默和被动模式。未知旧版本允许修复。主程序或媒体工具正在运行时提示正常退出后重试，静默模式返回非零；不强制终止视频任务。资源复制、WebView2、快捷方式和卸载入口继续使用上游逻辑。直接覆盖不提供断电或磁盘写入失败后的整包事务回滚。

升级 Tauri CLI 时应对照上游模板同步修改，尤其检查模板参数、资源路径、签名和卸载逻辑。不要重新引入维护页或升级时卸载步骤。`.github/workflows/windows-x64.yml` 使用真实安装器、Windows UI Automation 和原生按钮事件验证发布的 `beta.4` 升级、重装、损坏记录修复、降级保护及运行中保护，并保存 `installer-upgrade.json`；普通用户电脑上的桌面验收另行记录。

2026-10-12 的 [Windows 原生运行](https://github.com/mailingfeng/video-editor/actions/runs/38230808241)已完成 12 个安装与保护场景、安装资源检查及 14 项安装后媒体回归。核对三个程序的完整哈希，历史目录测试文件、保存输出测试文件和安装目录中其他用户文件均保留；中文／空格安装路径及开始菜单目标保持一致。证据见 [验证记录](../../docs/evidence/windows-installer-upgrade-20261012.json)。

验证脚本使用 `IShellLinkW` 读取快捷方式，避免旧的 `WScript.Shell` 接口在该 runner 对中文路径返回空值；独立探测确认旧快捷方式本身正确，生成逻辑未改变。主程序的期望哈希包含 Tauri 打包时的 `UNK` → `NSS` 标记修改；原构建文件在打包后会恢复，不能直接与安装文件比较。
