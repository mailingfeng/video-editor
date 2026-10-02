# Windows x64 开发包准备

Windows x64 的 FFmpeg／ffprobe 锁定资源与安装配置已准备。当前主机是 Intel Mac，没有执行 Windows 本机编译和安装，因此还没有 Windows 安装包，发行状态为 **NOT VERIFIED**。本次只推进 Windows x64。

## 锁定资源

采用 [Gyan 9.0.2 essentials 静态构建](https://github.com/GyanD/codexffmpeg/releases/tag/9.0.2)，[FFmpeg 官方下载页](https://ffmpeg.org/download.html)列出该提供者；[构建说明](https://www.gyan.dev/ffmpeg/builds/)说明它包含 libx264，并要求 Windows 10 或更高版本。本项目拟在 Windows 10／11 x64 验收，实际支持范围仍需实测。

| 文件 | SHA-256 |
| --- | --- |
| ffmpeg-9.0.2-essentials_build.zip | `60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba` |
| ffmpeg.exe | `3256173f3f8bffd7df12227c68adf68025edb1832273a9530688a7bb1ed8edec` |
| ffprobe.exe | `f0d36ecbbdd3bcfac3efa078c96c7271c2e68b3810595552ac3b7f17e9a65c52` |

两个 EXE 的 PE machine 均为 `0x8664`，锁定版本令牌为 `9.0.2-essentials_build-www.gyan.dev`。版本和构建参数来自 README／PE 内嵌字符串，没有冒充原生执行输出。实测记录见 [资源证据](evidence/windows-x64-resources-20261002.json)；下载链接、包内条目、工具哈希和许可材料登记于 `tools/sidecars.lock.json`。

## GitHub Actions

`.github/workflows/windows-x64.yml` 使用 Windows 2022 runner、Node.js 24、Rust 1.90.0 和固定提交的 [tauri-apps/tauri-action](https://github.com/tauri-apps/tauri-action)。推送 `ci/windows-x64` 分支会触发；工作流进入默认分支后也可在 Actions 页面手动运行。

流程依次检查锁定工具、Node／前端／后端测试、生成媒体回归，构建 NSIS，运行桌面 clippy，再静默安装到中文／空格目录。安装资源检查原生执行工具并核对哈希；后续回归通过 `VIDEO_EDITOR_INSTALLED_EXE` 使用安装目录的工具。原视频存在时再执行 2713 帧原样本转换与取消回归；没有上传该视频时，记录原样本 Windows 验证仍待执行。

通过检查后，Actions artifact `frameshift-<版本>-windows-x64-development-<运行ID>` 提供安装器、SHA-256 和安装资源检查 JSON；验证日志另存一个 artifact，保留 14 天。构建使用 GitHub 自动颁发的 `GITHUB_TOKEN`，权限为 contents: read；本地 PAT 仅用于提交代码和读取构建结果，不放入代码、工作流或日志。工作流不创建 GitHub Release。

仓库是公开的，上传代码前需确保整个待推送历史没有 `.env` 凭据。若旧本地历史已有 token，应从远端安全基线创建仅含代码的构建分支；删除 HEAD 文件或增加 `.gitignore` 不能清除祖先提交中的凭据。样本视频只有在允许公开时才加入该分支。

CI 的构建、静默安装和媒体回归可证明该 runner 上的行为；用户桌面启动、无开发工具环境、上传、完整发行材料和签名仍需独立验收，releaseStatus 保持 NOT VERIFIED。

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

GPL v3 文本与构建 README 已加入 Windows 包资源。所有对应源代码、外部库／应用依赖声明和 Authenticode 签名仍待准备；相关状态见 [第三方材料记录](third-party-notices.md)。当前资源准备证据不能替代 Windows 原生构建、安装或上传验收。
