# Windows x64 开发安装包

Windows x64 开发安装器已通过 [GitHub Actions 原生构建](https://github.com/mailingfeng/video-editor/actions/runs/37069401427)、静默安装、安装后转换与三项生命周期回归。产物为 `帧序_0.1.0_x64-setup.exe`，约 56.46 MiB。主程序、媒体工具和安装器实测均未签名；用户桌面和完整发行验收仍待完成，发行状态为 **NOT VERIFIED**。本次只推进 Windows x64。

Actions 页面提供 `frameshift-0.1.0-windows-x64-development-37069401427` 安装包 artifact，保留至 2026-10-16（UTC）；再次推送构建分支可重新生成。安装器 SHA-256 为 `6dca5b8931350022ebf62bfd98f2f830799ea0f4fa648304862cd7c3f7273959`。artifact 附带[无需开发工具的桌面验收指南](windows-desktop-acceptance.md)、构建元数据与资源记录。详情见[生命周期验收记录](evidence/windows-x64-lifecycle-actions-20261003.json)与[本次安装资源记录](evidence/windows-x64-lifecycle-installed-resources-20261003.json)；[首次构建记录](evidence/windows-x64-actions-20261003.json)保留不变。

## 锁定资源

采用 [Gyan 9.0.2 essentials 静态构建](https://github.com/GyanD/codexffmpeg/releases/tag/9.0.2)，[FFmpeg 官方下载页](https://ffmpeg.org/download.html)列出该提供者；[构建说明](https://www.gyan.dev/ffmpeg/builds/)说明它包含 libx264，并要求 Windows 10 或更高版本。本项目拟在 Windows 10／11 x64 验收，实际支持范围仍需实测。

| 文件 | SHA-256 |
| --- | --- |
| ffmpeg-9.0.2-essentials_build.zip | `60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba` |
| ffmpeg.exe | `3256173f3f8bffd7df12227c68adf68025edb1832273a9530688a7bb1ed8edec` |
| ffprobe.exe | `f0d36ecbbdd3bcfac3efa078c96c7271c2e68b3810595552ac3b7f17e9a65c52` |

两个 EXE 的 PE machine 均为 `0x8664`，锁定版本令牌为 `9.0.2-essentials_build-www.gyan.dev`。此前 [静态资源证据](evidence/windows-x64-resources-20261002.json)来自 README／PE 字符串；本次 [安装资源记录](evidence/windows-x64-installed-resources-20261003.json)原生执行了 `-version`／`-buildconf`／`-L`，确认完整版本、libx264、许可输出与上述 SHA。下载链接、包内条目、工具哈希和许可材料登记于 `tools/sidecars.lock.json`。

## GitHub Actions

`.github/workflows/windows-x64.yml` 使用 Windows 2022 runner、Node.js 24、Rust 1.90.0 和固定提交的 [tauri-apps/tauri-action](https://github.com/tauri-apps/tauri-action)。推送 `ci/windows-x64` 分支会触发；工作流进入默认分支后也可在 Actions 页面手动运行。

流程依次检查锁定工具、Node／前端／后端测试、生成媒体回归，构建 NSIS，运行桌面 clippy，再静默安装到中文／空格目录。安装资源检查原生执行工具并核对哈希；后续回归通过 `VIDEO_EDITOR_INSTALLED_EXE` 使用安装目录的工具。原视频存在时再执行 2713 帧原样本转换与取消回归；没有上传该视频时，记录原样本 Windows 验证仍待执行。

通过检查后，Actions artifact `frameshift-<版本>-windows-x64-development-<运行ID>` 提供安装器、SHA-256、安装资源检查 JSON 和桌面验收指南；验证日志另存一个 artifact，保留 14 天。构建使用 GitHub 自动颁发的 `GITHUB_TOKEN`，权限为 contents: read；本地 PAT 仅用于提交代码和读取构建结果，不放入代码、工作流或日志。工作流不创建 GitHub Release。

仓库是公开的，上传代码前需确保整个待推送历史没有 `.env` 凭据。若旧本地历史已有 token，应从远端安全基线创建仅含代码的构建分支；删除 HEAD 文件或增加 `.gitignore` 不能清除祖先提交中的凭据。样本视频只有在允许公开时才加入该分支。

CI 的构建、静默安装和媒体回归可证明该 runner 上的行为；用户桌面启动、无开发工具环境、上传、完整发行材料和签名仍需独立验收，releaseStatus 保持 NOT VERIFIED。

首次验证运行的源提交为 `dba94158a28dfe760ef813146e0dddfe98966024`。Windows 2022 runner 上的工具／检查器测试 17 项、前端 9 项、后端 42 项通过，4 项原生媒体测试默认忽略；生成媒体回归和使用安装目录工具的回归分别单独通过。后者覆盖中文／空格／引号路径、带音频与无音频、时间偏移、全范围视频及不支持／损坏输入。

后续运行 `37069401427` 基于 `720923a8d5b7af3e8c6941d83a556618376874a5`，工具／检查器 17 项、前端 9 项、后端 42 项通过；默认忽略 8 项，媒体回归另行显式执行。新增三项安装后回归通过（14.90 秒）：等待 FFmpeg 真实进度和非空临时媒体后取消、在登记的正式路径注入重名文件并确认不覆盖、强制结束自有测试服务及工具进程树后运行启动恢复并保留其他文件。取消后还完成一次新转换，源 identity 均保持不变。取消／崩溃测试仅在测试包装器加入 `-re`，让输入按实时速度读取；产品参数不变。这些验证生产后端和安装工具，不能填写用户桌面启动或干净主机收据，也未验证孤立 FFmpeg 仍运行时的恢复。

提供的原视频在 Intel Mac 完成真实转换和取消回归：2713 帧、AAC 48 kHz、源文件 identity 保持不变，取消后没有正式输出。原视频仍保留本地，本次 Windows runner 使用生成样例；原视频的 Windows 验证待允许公开上传后执行。

## 原生构建环境

在 Windows x64 安装 Node.js 24／npm 11、Rustup，并采用 x64 MSVC Rust 工具链。安装 Microsoft C++ Build Tools 的“Desktop development with C++”工作负载，以及 WebView2 Runtime，按 [Tauri Windows 前置依赖](https://v2.tauri.app/start/prerequisites/#windows)配置。仓库的 `rust-toolchain.toml` 固定 Rust 1.90.0。

从本分支代码根目录执行以下 PowerShell 命令；每条命令成功后再继续，失败时保留输出供排查。

```powershell
rustup target add x86_64-pc-windows-msvc
npm ci
npm run prepare:sidecars -- --target x86_64-pc-windows-msvc
npm run test:sidecars
npm run test:release
npm run test -- --run
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features desktop -- -D warnings
npm run test:media
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis
```

安装器应生成于 `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`。Windows 平台配置使用当前用户安装、中文／英文界面和 WebView2 下载模式。需要让目标电脑离线安装 WebView2 时，在有网络的构建机执行：

```powershell
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis --config src-tauri/tauri.windows.offline.conf.json
```

两种 WebView2 模式遵循 [Tauri Windows 安装文档](https://v2.tauri.app/distribute/windows-installer/)。离线模式准备的是 WebView2 安装器，不代表构建过程不需要网络。

准备脚本生成 `src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe` 和对应 ffprobe；安装时 Tauri 将它们放在主程序旁并去掉目标后缀。脚本核对包与二进制 SHA、PE 架构，只提取锁定条目，不覆盖损坏或版本不符的现有工具。网络较慢时可自行下载上述固定 ZIP，保持文件名放入 `.sidecar-cache/`，或使用 `npm run prepare:sidecars -- --target x86_64-pc-windows-msvc --archive-dir C:\path\to\cache`；缓存仍须通过完整 SHA 检查。

## 安装后的验证

从生成的安装器安装，再从安装目录启动应用。开发包可先执行以下检查，将示例路径替换为真实安装器和安装目录；记录文件须使用尚不存在的路径。

```powershell
node scripts/verify-release.mjs --target x86_64-pc-windows-msvc --artifact "C:\packages\setup.exe" --installed-dir "C:\installed\帧序" --development --record "C:\evidence\windows-resources.json"
```

该检查会原生执行包内工具的 `-version`／`-buildconf`／`-L`，核对完整版本令牌、libx264、架构与 SHA，并输出安装文件 fingerprint。`--development` 的成功状态仅为 DEVELOPMENT CHECKED，未签名或材料不全时仍为 NOT VERIFIED；安装器文件本身不足以证明安装通过。

另在没有 Node／Rust／系统 FFmpeg 的目标机器实测安装启动、工具自检、转换、取消、中文／空格路径、重名输出拒绝，以及异常退出后的自有临时目录清理。检查原视频 SHA 未变、输出帧数保持、AAC 48 kHz、完整校验通过且结果定位正确；取消时没有正式输出，已有重名文件不被覆盖。样本验收见 [发行验证记录](release-verification.md)。

实测后创建安装收据，填写 `target`、前述检查得到的 `fingerprint`、`os`、`cpu` 和 ISO 时间 `testedAt`。仅对真实通过的项目，把 `cases` 中 `installedLaunch`、`noDevelopmentTools`、`conversion`、`cancel`、`unicodeAndSpaces`、`outputConflict`、`startupRecovery` 设为 `true`。然后使用 `--receipt` 提供收据并去掉 `--development`，检查完整发行条件；材料和签名缺失时应失败。

GPL v3 文本与构建 README 已加入并在 Windows 安装资源中核对。所有对应源代码、外部库／应用依赖声明和 Authenticode 签名仍待准备；相关状态见 [第三方材料记录](third-party-notices.md)。Windows CI 的开发包检查不能替代用户桌面与平台上传验收。
