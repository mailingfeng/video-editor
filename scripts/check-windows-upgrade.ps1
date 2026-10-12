param(
  [Parameter(Mandatory)][string]$Installer,
  [Parameter(Mandatory)][string]$Installed,
  [Parameter(Mandatory)][string]$Evidence
)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'This test changes installation records and must run on a disposable GitHub runner.' }
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class InstallerButtons {
  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")]
  public static extern bool IsWindowEnabled(IntPtr window);
  [DllImport("user32.dll")]
  public static extern IntPtr GetParent(IntPtr window);
  [DllImport("user32.dll")]
  public static extern int GetDlgCtrlID(IntPtr window);
}
'@
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\帧序'
$config = Get-Content -Raw src-tauri/tauri.conf.json | ConvertFrom-Json
$version = $config.version
$exe = Join-Path $Installed 'video-editor.exe'
$payload = @{}
foreach ($name in @('video-editor.exe', 'ffmpeg.exe', 'ffprobe.exe')) {
  $payload[$name] = (Get-FileHash -LiteralPath (Join-Path 'src-tauri/target/x86_64-pc-windows-msvc/release' $name) -Algorithm SHA256).Hash
}
$cases = [ordered]@{}
$report = [ordered]@{
  sourceCommit = $env:GITHUB_SHA
  version = $version
  baselineVersion = '0.0.1-beta.4'
  baselineSha256 = '861b770592c54c8e0f8be02dfa26a660035a105d77ac09ee8f6a39c27ce5b810'
  cases = $cases
}

function Invoke-SilentInstaller([string]$File, [string]$Arguments, [bool]$Success = $true) {
  $p = Start-Process -FilePath $File -ArgumentList $Arguments -PassThru
  try {
    if (-not $p.WaitForExit(120000)) { throw 'Silent installer timed out.' }
    if ($Success -ne ($p.ExitCode -eq 0)) { throw "Unexpected installer exit code: $($p.ExitCode)" }
    return $p.ExitCode
  } finally {
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
  }
}

# Drive the real wizard, including Next/Install/Finish. Never select an
# uninstall option. A baseline radio page demonstrates the reported problem.
function Invoke-InstallerWizard([string]$File, [string]$Expected = 'complete', [string]$Arguments = '') {
  $start = @{ FilePath = $File; PassThru = $true }
  if ($Arguments) { $start.ArgumentList = $Arguments }
  $p = Start-Process @start
  $stages = [System.Collections.Generic.List[string]]::new()
  $observed = ''
  $lastControls = @()
  $deadline = (Get-Date).AddMinutes(3)
  try {
    while ((Get-Date) -lt $deadline) {
      $p.Refresh()
      if ($p.HasExited) {
        if ($Expected -eq 'complete' -and $p.ExitCode -ne 0) { throw "Wizard failed: $($p.ExitCode)" }
        if ($Expected -ne 'complete' -and $observed -ne $Expected) { throw "Expected $Expected, observed $observed" }
        return @{ result = $Expected; exitCode = $p.ExitCode; stages = @($stages); controls = $lastControls }
      }
      $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
      $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)
      foreach ($window in $windows) {
        $nodes = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        $names = @($nodes | ForEach-Object { $_.Current.Name }) -join ' | '
        if ($names -and ($stages.Count -eq 0 -or $stages[$stages.Count - 1] -ne $names)) { $stages.Add($names) }
        $radios = @($nodes | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::RadioButton })
        if ($radios.Count -gt 0) {
          if ($Expected -ne 'maintenance-choice') { throw "Unexpected uninstall/maintenance choice: $names" }
          Stop-Process -Id $p.Id -Force
          $p.WaitForExit()
          return @{ result = 'maintenance-choice'; stages = @($stages) }
        }
        if ($names -match '已安装较新的版本|newer version is already installed') { $observed = 'downgrade-blocked' }
        if ($names -match '请先退出帧序|Please close FrameShift') { $observed = 'running-blocked' }
        if ($observed -and $observed -ne $Expected) { throw "Unexpected prompt: $names" }
        $buttons = @($nodes | Where-Object {
          $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and
            [InstallerButtons]::IsWindowEnabled([IntPtr]$_.Current.NativeWindowHandle) -and
            [InstallerButtons]::IsWindowVisible([IntPtr]$_.Current.NativeWindowHandle)
        })
        $lastControls = @($nodes | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button } | ForEach-Object {
          @{ name = $_.Current.Name; enabled = $_.Current.IsEnabled; offscreen = $_.Current.IsOffscreen; handle = $_.Current.NativeWindowHandle }
        })
        $finish = @($buttons | Where-Object { $_.Current.Name -match '完成|Finish' })
        if ($finish.Count -gt 0) {
          # Do not start the application or create a desktop shortcut at Finish.
          foreach ($node in $nodes) {
            if ($node.Current.ControlType -eq [System.Windows.Automation.ControlType]::CheckBox) {
              # NSIS reads native checkbox state at Finish. Set it directly;
              # UI Automation Invoke/Toggle can miss a reused wizard control.
              [InstallerButtons]::SendMessage([IntPtr]$node.Current.NativeWindowHandle, 0x00F1, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
            }
          }
          $button = $finish[0]
        } elseif ($observed) {
          $button = $buttons | Where-Object { $_.Current.Name -match '取消|Cancel|确定|OK' } | Select-Object -First 1
        } else {
          $button = $buttons | Where-Object { $_.Current.Name -match '下一步|Next|^&?安装|^&?Install|确定|^&?OK$' } | Select-Object -First 1
        }
        if ($button) {
          $handle = [IntPtr]$button.Current.NativeWindowHandle
          $parent = [InstallerButtons]::GetParent($handle)
          $id = [InstallerButtons]::GetDlgCtrlID($handle)
          # BM_CLICK can fail when a dialog is inactive (Microsoft Win32 docs).
          # Deliver its BN_CLICKED notification to the native dialog directly.
          if ($handle -eq [IntPtr]::Zero -or $parent -eq [IntPtr]::Zero -or $id -le 0 -or
              -not [InstallerButtons]::PostMessage($parent, 0x0111, [IntPtr]$id, $handle)) { throw 'Native wizard button notification failed.' }
        }
      }
      Start-Sleep -Milliseconds 400
    }
    throw "Installer wizard timed out. Last stage: $($stages | Select-Object -Last 1); Controls: $($lastControls | ConvertTo-Json -Compress)"
  } finally {
    $p.Refresh()
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
  }
}

function Assert-Installation {
  if ((Get-ItemProperty $uninstallKey).DisplayVersion -ne $version) { throw 'Installed version was not updated.' }
  if (-not (Test-Path -LiteralPath (Join-Path $Installed 'uninstall.exe'))) { throw 'New uninstaller missing.' }
  foreach ($entry in $payload.GetEnumerator()) {
    if ((Get-FileHash -LiteralPath (Join-Path $Installed $entry.Key) -Algorithm SHA256).Hash -ne $entry.Value) { throw "Installed payload differs: $($entry.Key)" }
  }
  if (-not (Test-Path -LiteralPath (Join-Path $Installed 'licenses/windows-x64/Tauri-installer-MIT.txt'))) { throw 'Installer license missing.' }
  foreach ($entry in $preserved.GetEnumerator()) {
    if ((Get-FileHash -LiteralPath $entry.Key -Algorithm SHA256).Hash -ne $entry.Value) { throw "User file changed: $($entry.Key)" }
  }
  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Programs')) '帧序.lnk'))
  if ($shortcut.TargetPath -ne $exe) { throw 'Start menu shortcut no longer points to the original directory.' }
}

try {
  if (Test-Path $uninstallKey) { throw 'Expected a clean runner without an existing installation.' }
  $baseline = Join-Path $env:RUNNER_TEMP 'frameshift-beta4-setup.exe'
  Invoke-WebRequest 'https://github.com/mailingfeng/video-editor/releases/download/v0.0.1-beta.4/frameshift_0.0.1-beta.4_windows_x64_setup.exe' -OutFile $baseline
  if ((Get-FileHash $baseline -Algorithm SHA256).Hash.ToLowerInvariant() -ne $report.baselineSha256) { throw 'Baseline installer checksum mismatch.' }
  Invoke-SilentInstaller $baseline "/S /D=$Installed" | Out-Null
  if ((Get-ItemProperty $uninstallKey).DisplayVersion -ne '0.0.1-beta.4') { throw 'Baseline version mismatch.' }
  $cases.baselineMaintenance = Invoke-InstallerWizard $baseline 'maintenance-choice'

  $data = Join-Path (Join-Path $env:APPDATA $config.identifier) 'job-records'
  New-Item -ItemType Directory -Force (Join-Path $data 'history') | Out-Null
  $history = Join-Path $data 'history\upgrade-preservation.json'
  $userFile = Join-Path $Installed '用户保留文件.txt'
  $output = Join-Path $env:RUNNER_TEMP '已保存视频.mp4'
  [IO.File]::WriteAllText($history, '{"upgradeFixture":"preserve exactly"}')
  [IO.File]::WriteAllText($userFile, 'User file must survive upgrade and repair')
  [IO.File]::WriteAllText($output, 'Saved output fixture; not a media validation sample')
  $preserved = @{}
  foreach ($file in @($history, $userFile, $output)) { $preserved[$file] = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash }

  $cases.upgradeFromBeta4 = Invoke-InstallerWizard $Installer
  Assert-Installation
  $installedHash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
  $cases.reinstallSameVersion = Invoke-InstallerWizard $Installer
  Assert-Installation
  if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $installedHash) { throw 'Reinstall payload differs.' }

  $publisher = (Get-ItemProperty $uninstallKey).Publisher
  $remembered = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\$publisher\帧序", $true)
  try {
    if ($remembered.GetValue('') -ne $Installed) { throw 'Remembered install location mismatch.' }
    $remembered.DeleteValue('')
  } finally { $remembered.Dispose() }
  $cases.repairMissingLocationKey = Invoke-InstallerWizard $Installer
  Assert-Installation

  Remove-Item -LiteralPath (Join-Path $Installed 'uninstall.exe')
  Remove-ItemProperty -Path $uninstallKey -Name DisplayVersion
  $cases.repairMissingUninstallerAndVersion = Invoke-InstallerWizard $Installer
  Assert-Installation

  Set-ItemProperty $uninstallKey DisplayVersion '99.0.0'
  $cases.guiDowngrade = Invoke-InstallerWizard $Installer 'downgrade-blocked'
  $cases.silentDowngrade = @{ exitCode = (Invoke-SilentInstaller $Installer '/S' $false) }
  $cases.passiveDowngrade = Invoke-InstallerWizard $Installer 'downgrade-blocked' '/P'
  if ((Get-ItemProperty $uninstallKey).DisplayVersion -ne '99.0.0') { throw 'Downgrade changed the version record.' }
  if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $installedHash) { throw 'Downgrade changed the app.' }
  Set-ItemProperty $uninstallKey DisplayVersion $version
  Assert-Installation

  $app = Start-Process -FilePath $exe -PassThru
  try {
    Start-Sleep -Seconds 5
    $app.Refresh()
    if ($app.HasExited) { throw 'Installed app exited before the running-app check.' }
    $cases.runningApp = Invoke-InstallerWizard $Installer 'running-blocked'
    $cases.silentRunningApp = @{ exitCode = (Invoke-SilentInstaller $Installer '/S' $false) }
    $app.Refresh()
    if ($app.HasExited) { throw 'Installer terminated the running app.' }
    if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $installedHash) { throw 'Running app was overwritten.' }
    if (-not $app.CloseMainWindow() -or -not $app.WaitForExit(15000)) { throw 'App did not close normally.' }
  } finally {
    $app.Refresh()
    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force }
  }
  $cases.silentRepair = @{ exitCode = (Invoke-SilentInstaller $Installer '/S') }
  $cases.passiveRepair = Invoke-InstallerWizard $Installer 'complete' '/P'
  Assert-Installation
  $report.result = 'passed'
} catch {
  $report.error = $_.Exception.Message
  throw
} finally {
  $report.checkedAt = (Get-Date).ToUniversalTime().ToString('o')
  $report | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 $Evidence
}
