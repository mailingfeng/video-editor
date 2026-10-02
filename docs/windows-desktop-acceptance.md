# Windows x64 桌面验收

在 Windows 10／11 x64 的普通用户账户运行。安装与以下检查不需要 Node、Rust 或系统 FFmpeg；检查使用安装目录内的 `ffmpeg.exe`、`ffprobe.exe`。GitHub runner 的安装后回归验证后端和工具，桌面窗口、文件对话框与干净电脑仍由本流程实测。

从同一次 Actions artifact 解压安装器、`build.json`、`installed-resources.json` 和本指南。用 PowerShell 的 `Get-FileHash -Algorithm SHA256` 核对安装器，必须等于 `build.json` 的 `installerSha256`。该开发包尚未签名，正式发行状态仍为 NOT VERIFIED。

## 准备与首次启动

记录 Windows 版本、CPU、时间与安装目录。运行 `Get-Command node,cargo,ffmpeg,ffprobe -ErrorAction SilentlyContinue`，记录找到的程序；有开发工具的机器不能填写 `noDevelopmentTools: true`。在干净机器上从安装器安装，再从开始菜单启动“帧序”，确认窗口显示、工具检查无错误、导入和目录选择对话框正常。

选择带中文和空格的源路径、输出目录，例如 `C:\验收 视频\原视频.mp4` 与 `C:\验收 输出`。关闭其他“帧序”实例，本轮只运行一个任务。处理前在 PowerShell 保存源文件 SHA，处理完成后再检查：

```powershell
$source = 'C:\验收 视频\原视频.mp4'
$sourceBefore = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
```

提供的原视频预期 SHA 为 `c4fd9bc7b1c8fc928848808aee0ffd7b0c961fffb449f92587f3c756211d8beb`，共 2713 帧、720×1280。原视频只需留在验收电脑，无需公开上传代码仓库。

## 转换与输出

导入源视频，选择基础转换与独立输出目录，开始任务。确认状态依次进入处理、校验、保存，只有完整校验通过后显示成功；点击“显示结果”应选中本次输出。记录任务 ID、输出路径和任务日志。将下面路径替换成实际安装目录和本次输出，然后使用包内工具复核：

```powershell
$installed = 'C:\实际安装目录\帧序'
$result = 'C:\验收 输出\本次实际输出.mp4'
if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $sourceBefore) {
  throw '源视频发生变化'
}
& (Join-Path $installed 'ffprobe.exe') -v error -count_frames -show_streams -show_format -of json $result
if ($LASTEXITCODE -ne 0) { throw '输出探测失败' }
& (Join-Path $installed 'ffmpeg.exe') -v error -xerror -i $result -map 0:v:0 -map '0:a:0?' -f null NUL
if ($LASTEXITCODE -ne 0) { throw '输出完整解码失败' }
```

保存探测输出，核对主视频 H.264、相同尺寸／帧率／帧数，音轨存在时为 AAC 48 kHz。原样本输出必须为 2713 帧。独立播放首段、中段和结尾，检查画面、声音与同步。全解码成功不代表平台的重复内容判断发生变化。

## 取消与重名保护

再次开始转换，等待进度实际增长后点击取消。确认最终状态为已取消，本次任务没有正式输出，自有临时目录已经清理，源文件 SHA 未变。随后再次转换应成功，证明取消后可以继续使用。

重名故障注入使用当前任务记录的准确 `workspace.finalPath`，不是猜测文件名。任务运行时在 `%APPDATA%\com.frameshift.videoeditor\job-records` 找到本次 `<任务ID>.json`，记录 `workspace.directory`、`tempPath` 与 `finalPath`。在 `finalPath` 独占创建一个测试文件，保存它的 SHA；不要替换已有用户文件。可以用以下 PowerShell 命令独占创建，路径必须来自本次任务记录：

```powershell
$conflict = '从本次记录复制 workspace.finalPath'
$file = [System.IO.File]::Open($conflict, [System.IO.FileMode]::CreateNew)
try {
  $bytes = [System.Text.Encoding]::UTF8.GetBytes('验收已有文件，不应被覆盖')
  $file.Write($bytes, 0, $bytes.Length)
} finally { $file.Dispose() }
$conflictBefore = (Get-FileHash -LiteralPath $conflict -Algorithm SHA256).Hash
```

任务应在保存时报告重名冲突，已有测试文件 SHA 保持不变，临时目录被清理。若任务已完成，换新任务重试；不要把注入时机错过记为通过。检查失败时保留日志和目录，便于排查。

## 异常退出恢复

在独立测试任务实际产生进度后，保存本次任务记录。通过任务管理器或 `Get-CimInstance Win32_Process` 核实本次安装应用的 PID、路径，以及其 FFmpeg 子进程的 `ParentProcessId` 和安装路径。仅强制结束这些已核实归属的测试进程；这是故障注入，当前任务会中断。

确认两个进程都已结束，任务记录和临时媒体仍存在，正式输出尚未发布。重新从开始菜单启动应用；自有临时目录和活动记录应清理，原视频、之前成功输出、重名测试文件及其他用户文件保持不变。这个检查覆盖应用和子进程都结束后的恢复；孤立 FFmpeg 仍运行时的恢复另行记录，不能推导为通过。

## 保存验收结果

将下面 JSON 保存为本机验收收据，`fingerprint` 必须使用同一安装包附带的 `installed-resources.json` 值；填写真实 OS／CPU／时间，只有实测通过才把相应项改成 `true`。不要提交本机路径、视频或账户信息到公开仓库。签名和完整分发材料缺失时，桌面收据不能把正式发行状态改为通过。

```json
{
  "target": "x86_64-pc-windows-msvc",
  "fingerprint": "复制同一次 installed-resources.json 的 fingerprint",
  "os": "填写实际 Windows 版本与构建号",
  "cpu": "填写实际 x64 CPU",
  "testedAt": "填写真实 ISO 8601 时间",
  "cases": {
    "installedLaunch": false,
    "noDevelopmentTools": false,
    "conversion": false,
    "cancel": false,
    "unicodeAndSpaces": false,
    "outputConflict": false,
    "startupRecovery": false
  }
}
```

平台兼容性单独记录：在有权限使用该视频的抖音或小红书账户手动上传本次输出，记录平台、时间和上传结果；公开发布由账户持有人决定。未实测时保持待验收，正常上传成功也不能证明重复内容判定改变。
