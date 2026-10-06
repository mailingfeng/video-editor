# 桌面发行验证

| 原生目标 | 发行验收 |
| --- | --- |
| macOS Intel / x86_64-apple-darwin | NOT VERIFIED |
| macOS Apple Silicon / aarch64-apple-darwin | NOT VERIFIED |
| Windows x64 / x86_64-pc-windows-msvc | NOT VERIFIED |

当前阶段建立发行验证器与开发安装包，不把开发环境的编译成功记为发行通过。原生安装、签名、公证、媒体工具与许可材料的证据需要分别记录。

2026-10-03 已补齐 Windows x64 的 CI 原生开发安装包验证，见下方日期化记录与 [Windows 安装包说明](windows-x64.md)。三个目标的正式发行状态仍保持 NOT VERIFIED。

## 构建与检查

在匹配目标的原生主机先执行 `npm ci`、`npm run prepare:sidecars` 和全部测试。准备脚本对没有校验资源的目标明确失败；不要改名复用其他架构的二进制。

```sh
# Windows x64，默认下载 WebView2 bootstrapper
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis
# 离线 WebView2 模式
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis --config src-tauri/tauri.windows.offline.conf.json
# 两种 Mac 分别在对应机器执行
npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg
npm run tauri -- build --target x86_64-apple-darwin --bundles app,dmg
```

Windows 配置采用当前用户安装、中文／英文界面、阻止版本降级；离线模式在构建时取得 WebView2 离线安装器。[Tauri Windows 安装文档](https://v2.tauri.app/distribute/windows-installer/)说明了两种模式。Mac 最低版本配置为 12.0，实际支持范围仍需原生验收。

```sh
# 本地开发包资源检查，不授予发行通过状态
node scripts/verify-release.mjs --target x86_64-apple-darwin --artifact /path/to/帧序.app --development --record /path/to/new-record.json
# 发行检查：验证工具、材料、签名、公证与安装记录
node scripts/verify-release.mjs --target x86_64-apple-darwin --artifact /path/to/installed/帧序.app --receipt /path/to/receipt.json --record /path/to/new-release-record.json
# Windows 必须提供安装器和已经安装的目录
node scripts/verify-release.mjs --target x86_64-pc-windows-msvc --artifact /path/to/setup.exe --installed-dir /path/to/installed --receipt /path/to/receipt.json
npm run test:release
```

检查器只接受匹配的本机目标；读取 Mach-O／PE 架构，核对主程序与两个媒体工具的架构、执行权限、锁定 SHA 与版本，并用包内绝对路径执行工具。Mac 发行检查要求主程序与工具的 Developer ID 签名、完整包签名、Gatekeeper assessment 和 stapled notarization；Windows 要求主程序、工具与安装器的有效 Authenticode。签名后工具字节发生变化时，另记 `bundledBinarySha256`，保留原下载的 `binarySha256`。[Tauri Mac 签名文档](https://v2.tauri.app/distribute/sign/macos/)记录了签名和公证准备。

输出 JSON 记录本机 OS、CPU 目标、时间、工具版本／构建参数／许可输出、校验和、包 fingerprint、签名和错误。`--development` 成功仅为 `DEVELOPMENT CHECKED`，`releaseStatus` 始终 `NOT VERIFIED`。记录文件采用独占创建，避免覆盖已有证据。

安装记录须来自真实安装后的操作，`target` 与 `fingerprint` 必须对应检查产物，同时填写 `os`、`cpu`、`testedAt`，以及 `cases` 的 `installedLaunch`、`noDevelopmentTools`、`conversion`、`cancel`、`unicodeAndSpaces`、`outputConflict`、`startupRecovery`。只有实测通过才填写 `true`；不能从交叉编译、单元测试或本文推导这些值。

## 2026-10-02 本地结果

macOS Intel 的 `.app` 编译与资源检查已执行，两个包内媒体工具均为锁定的 9.0.2-tessus，架构与 SHA 匹配。没有 Developer ID 签名、公证和完整对应源代码／许可材料，因此普通发行检查应返回非零，发行状态保持 NOT VERIFIED。

开发 DMG 已通过 `hdiutil` 校验并只读挂载，应用复制到独立安装目录后运行。关闭 Vite 开发服务器，通过系统文件对话框导入中文／空格路径的样例并转换，结果为 36 帧、AAC 48 kHz；Finder 正确选中输出。提供的原视频也在安装应用中转换成功，保留 2713 帧和 720×1280，音频为 AAC 48 kHz；转换阶段取消后没有发布该任务输出，源文件 SHA 未变。正常关闭后重新启动，窗口与最新终态恢复成功。

重名测试在当前任务登记的正式输出路径独占创建测试标记；应用在保存阶段显示 `output_conflict`，标记 SHA 保持不变，临时目录与活动登记均已清理。此为已安装应用的故障注入检查，不是仅运行文件系统单元测试。

启动恢复测试在转换时强制结束本次安装应用及核实归属的 FFmpeg 子进程；重启前登记与临时目录仍存在，重启后均被清理，未发布该任务输出，也未改动同目录的重名测试文件。此检查覆盖两个进程均已结束的残留清理，未覆盖孤立 FFmpeg 仍运行时的重启。

证据：[构建资源检查](evidence/intel-development-package-20261002.json)、[安装副本资源检查](evidence/intel-installed-resources-20261002.json)、[本机操作记录](evidence/intel-installation-20261002.json)、[桌面截图](evidence/intel-installed-ui-20261002.png)和[安装应用的发行拒绝记录](evidence/intel-installed-release-rejection-20261002.json)。操作记录保留 `noDevelopmentTools: false`，不能作为完整发行收据使用。普通发行检查已使用该收据执行并返回 1；三平台 Task 8 仍未完成。

首次安装启动曾观察到进程存在但没有可访问窗口；主线程采样显示事件循环空闲，没有卡在任务恢复。随后直接启动与正常关闭后的多次 Finder 启动均成功；未定位第一次现象的原因，也没有据此修改产品代码。干净机器的首次启动仍需重复验收。当前主机装有 Node／Rust；开发服务器关闭不能代替“无开发工具环境”的发行验收。

最初 Tauri `.app,dmg` 构建在 DMG 脚本阶段失败；生成的打包脚本使用其公开的 `--skip-jenkins` 选项后可完成，跳过 Finder 外观配置。开发 DMG 使用同一 `.app` 和 Applications 快捷链接，仅作为本机开发产物。此结果没有证明原始 Finder 装饰步骤的具体失败原因。

本节 Intel 安装记录形成时，Windows x64 与 Apple Silicon 均没有已校验的工具对。下节记录后来补齐的 Windows 资源；两个平台均没有原生安装证据。FFmpeg 分发材料和应用依赖声明仍需补齐，详见 `third-party-notices.md`。

## 2026-10-02 Windows x64 资源准备

按本次范围仅补 Windows x64，采用固定版本的 Gyan 9.0.2 essentials ZIP。下载包 SHA 与 [发布者校验和](https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.zip.sha256)及 [GitHub 发布记录](https://github.com/GyanD/codexffmpeg/releases/tag/9.0.2)一致；两个可执行文件均为 PE x64。资源已写入 Tauri 的固定目标文件名，重复准备通过 SHA／架构复核，不替换已存在但不匹配的文件。GPL v3 和 README 为原包逐字节副本，并加入 Windows 资源配置。

证据见 [Windows 资源检查](evidence/windows-x64-resources-20261002.json)，复现步骤见 [Windows x64 开发包准备](windows-x64.md)。版本令牌和构建配置来自 README／PE 静态检查，尚未执行 Windows 工具；检查器支持完整版本令牌匹配和 Windows CRLF 输出，不使用前缀放行。资源准备与发行检查的 17 项测试、前端 9 项、后端默认 43 项及 TypeScript／Vite 构建、桌面 clippy 均通过，后端 4 项媒体原生测试本轮未重复执行。以上均运行于 Intel Mac；Windows 发行检查按预期因主机不匹配返回 1。

主线程复核确认：仅提取锁定 EXE，核对真实包与资源哈希，不跟随已有资源符号链接，不覆盖不匹配文件；Windows 版本输出按完整令牌匹配。第三方原样材料通过 `.gitattributes` 禁止换行转换，以保持 Windows 检出后的 SHA。按仓库要求由主线程复核，未使用独立审查代理；Windows 本机行为仍等待实测。

未生成 Windows NSIS 安装包，未执行安装、转换、取消、文件系统与签名的 Windows 原生验收；发行状态继续为 NOT VERIFIED。对应源代码、外部库和应用依赖的完整发行材料仍未齐。之前的 Intel 安装包包含当时的锁清单，本次未重建该包；其日期化证据保留原值。

## 2026-10-03 Windows x64 GitHub Actions

[运行 37040318798](https://github.com/mailingfeng/video-editor/actions/runs/37040318798)基于 `ci/windows-x64` 分支的 `dba94158a28dfe760ef813146e0dddfe98966024`，使用 Windows 2022、Node.js 24、Rust 1.90.0 和官方 Tauri Action，成功生成 NSIS 安装器。静默安装到中文／空格目录后，主程序与两个工具均为 PE x64，工具 SHA、完整版本、libx264 与许可资源一致；再用安装目录的实际工具完成生成媒体回归。

工具／发行检查器测试 17 项、前端 9 项、后端 42 项通过，后端 4 项原生媒体测试默认忽略；生成媒体回归以及安装后工具回归分别单独执行通过。真实子进程的三秒取消宽限测试在 Windows 完成；它使用真实时钟和有界等待，避免虚拟时钟与 Windows OS 管道混用导致停滞。

PowerShell 签名查询已实际执行，主程序、两个媒体工具和安装器均为 `NotSigned`；CI 对签名查询执行失败设独立检查。安装资源检查为 DEVELOPMENT CHECKED，正式发行仍为 NOT VERIFIED。此轮没有形成用户桌面启动、无开发工具环境或平台上传的安装收据。

原视频在本地 Intel Mac 的真实转换／取消回归通过，保留 2713 帧，输出 AAC 48 kHz，源文件 identity 未变，取消后无正式输出；本次未公开上传原视频，Windows 使用生成样例。证据见 [Windows 构建记录](evidence/windows-x64-actions-20261003.json)与[安装资源原始记录](evidence/windows-x64-installed-resources-20261003.json)。完整对应源代码、依赖材料和有效签名仍待补齐。

## 2026-10-03 Windows 安装后生命周期回归

[运行 37069401427](https://github.com/mailingfeng/video-editor/actions/runs/37069401427)在同一 Windows 2022 原生流程完成构建、静默安装和安装工具复测。新增取消、重名保护、异常退出后的自有残留恢复三项回归全部通过，安装后执行耗时 14.90 秒；转换回归、工具检查、前端／后端测试和桌面 clippy 同时通过。后端默认 42 项通过、8 项忽略，忽略的媒体测试按工作流另行显式执行，子进程辅助入口不作为验收项。

取消用例等待真实 FFmpeg 正进度及非空临时文件，确认取消后不发布、临时目录与登记清理、输入身份不变，并能再次完成转换。重名用例在记录的正式路径独占创建标记，确认报告冲突、标记内容不变。异常退出用例仅强制结束自有测试服务与其工具进程树，确认残留存在，再执行应用使用的启动恢复例程；只清理自有临时文件，保留源视频及其他文件。取消／异常退出用例通过测试包装器加入 `-re` 以确保有操作窗口，没有修改产品处理参数。

本机分别临时禁用清理、允许覆盖输出、跳过恢复清理时，对应三项回归均失败；恢复原代码后全部通过。原视频在 Intel Mac 使用更新后的媒体回归入口再次通过，保持 2713 帧、AAC 48 kHz 和源 identity，取消不发布；它仍未上传，不能作为 Windows 原样本证据。

见[本次构建与测试记录](evidence/windows-x64-lifecycle-actions-20261003.json)、[原始安装资源记录](evidence/windows-x64-lifecycle-installed-resources-20261003.json)及[桌面验收指南](windows-desktop-acceptance.md)。指南随本次安装包 artifact 提供，使用系统 PowerShell 和包内工具，收据各项默认 false。这些自动回归未启动真实 Windows 用户 UI，没有证明干净主机或平台上传，未覆盖孤立 FFmpeg 仍运行时的恢复；正式发行保持 NOT VERIFIED，Task 8 仍未完成。

## 2026-10-02 本地最终复核记录

本次自动检查：后端默认套件 43 项通过，4 项需要原生媒体资源的测试默认忽略；随后两项端到端媒体回归与另外两项原生探测／校验回归均单独通过。前端 9 项与发行检查器 7 项通过，TypeScript／Vite 构建、桌面 Rust clippy 和 Intel `.app` 构建通过。普通发行检查返回 1，符合当前未签名、材料与干净机器证据不完整的状态。

任务 1～7 已完成开发验证；任务 8 保持未完成。主线程按仓库要求复核了输入约束、转换参数、独占保存、取消／退出、版本快照和 UI 流程。新增回归确认：全范围 `yuvj420p` 解码别名可验证，范围改变仍失败；RGB 矩阵转换在首版明确拒绝；保存时打开可写的自有输出句柄，以满足 Windows FlushFileBuffers 要求。Windows 的文件系统行为仍需原生检查。

开发决策：采用参考页的顶部工具栏选择输出并启动，来源、预设和元数据留在左侧，确保 840px 高的默认窗口能直接开始；如需调整，只涉及布局，不改变任务接口。保留单文件 MVP，批量处理需要后续单独实现。前端订阅以任务 ID 与版本过滤；本地开发检查和发行状态分开，缺少主机、签名和材料时不能发行。日志有 64 KiB 上限；长期开启会话的任务数量暂未设置上限，可在后续增加保留策略。

重新导入另一份视频后，界面明确标注上次任务结果，避免把旧的成功状态误认为新输入已处理。主线程复核替代独立审查代理，遵循仓库 AGENTS.md 的顺序执行要求；代价是没有独立作者的复核。Windows 的文件系统检查延后至对应机器，当前测试结果不能代替该项。

接受 `yuvj420p` 需要来源明确为全范围 8 位 4:2:0，并同时核对输出范围；解码器改变时需重新检验这个等价关系。RGB 输入需要后续明确设计矩阵转换的预设，当前拒绝，以免标签与实际画面不一致。已知色彩字段逐项显示，部分缺失会明确提示。
