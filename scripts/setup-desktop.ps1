<#
.SYNOPSIS
  把 ClaudeGate 放上桌面，并把旧的一批按钮移进备份文件夹。

.DESCRIPTION
  两件事分开做，默认都只「预演」不动手，确认无误再加 -Apply。

  顺序很重要：**先确认 ClaudeGate 里那几项功能都能用，再移旧按钮。**
  那 7 个按钮目前是启动 Claude 的唯一合法入口 —— claude.exe 上有
  Deny ExecuteFile ACE，双击会被系统拒绝。移早了会把自己关在门外。

.EXAMPLE
  # 只看会做什么
  .\setup-desktop.ps1

.EXAMPLE
  # 只建快捷方式，先不动旧按钮
  .\setup-desktop.ps1 -Apply -SkipBackup

.EXAMPLE
  # 确认功能都正常之后，把旧按钮收走
  .\setup-desktop.ps1 -Apply
#>
[CmdletBinding()]
param(
    # 不加这个开关就只预演，什么都不改。
    [switch]$Apply,

    # 只建快捷方式，不动旧按钮。
    [switch]$SkipBackup,

    # ClaudeGate 可执行文件路径。默认找安装后的位置，找不到再找构建产物。
    [string]$ExePath
)

$ErrorActionPreference = 'Stop'
$desktop = [Environment]::GetFolderPath('Desktop')

# 这 7 个是旧按钮。清单写死是有意的 —— 绝不按通配符扫桌面，
# 那样会误伤到 "Turb GPT 一键开关" 之类同样带「一键」字样的无关项。
$oldButtons = @(
    '一键关闭所有克劳德.cmd',
    '一键升级克劳德.cmd',
    '一键启动克劳德代码.cmd',
    '一键启动克劳德桌面.cmd',
    '一键启动酒馆角色扮演.lnk',
    '切换Claude账户.lnk',
    '添加克劳德白名单地址.vbs'
)

function Resolve-ClaudeGateExe {
    if ($ExePath) {
        if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
            throw "指定的 ExePath 不存在：$ExePath"
        }
        return (Resolve-Path -LiteralPath $ExePath).Path
    }
    $repo = Split-Path -Parent $PSScriptRoot
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'ClaudeGate\ClaudeGate.exe'),
        (Join-Path ${env:ProgramFiles} 'ClaudeGate\ClaudeGate.exe'),
        (Join-Path $repo 'src-tauri\target\release\claude-gate.exe')
    )
    foreach ($c in $candidates) {
        if ($c -and (Test-Path -LiteralPath $c -PathType Leaf)) { return $c }
    }
    throw '找不到 ClaudeGate 可执行文件。先跑 npm run tauri build，或者用 -ExePath 指定。'
}

Write-Host '=== ClaudeGate 桌面部署 ===' -ForegroundColor Cyan
if (-not $Apply) {
    Write-Host '预演模式：只显示会做什么，不会改动任何文件。加 -Apply 才真的执行。' -ForegroundColor Yellow
}
Write-Host ''

# ---- 1. 建快捷方式 ----
$exe = Resolve-ClaudeGateExe
$lnk = Join-Path $desktop 'ClaudeGate.lnk'
Write-Host "[1] 桌面快捷方式" -ForegroundColor Cyan
Write-Host "    目标  $exe"
Write-Host "    落点  $lnk"

if ($Apply) {
    $shell = New-Object -ComObject WScript.Shell
    $sc = $shell.CreateShortcut($lnk)
    $sc.TargetPath = $exe
    $sc.WorkingDirectory = Split-Path -Parent $exe
    $sc.Description = 'Claude 环境控制面板：IP 锁 · 纯净度 · DNS 泄露 · 插件'
    $sc.IconLocation = "$exe,0"
    $sc.Save()
    Write-Host '    已创建。' -ForegroundColor Green
}
Write-Host ''

# ---- 2. 收走旧按钮 ----
if ($SkipBackup) {
    Write-Host '[2] 已跳过旧按钮处理（-SkipBackup）。' -ForegroundColor Yellow
    Write-Host '    建议先用几天，确认这几项在面板里都能用，再回来收：' -ForegroundColor Yellow
    Write-Host '    启动桌面端 / 启动 Claude Code / 一键关闭 / 升级 / 切换账户 / 白名单 / 启动酒馆'
    return
}

$stamp = Get-Date -Format 'yyyyMMdd'
$backupDir = Join-Path $desktop "old-claude-buttons-backup-$stamp"

Write-Host "[2] 旧按钮 -> $backupDir" -ForegroundColor Cyan
$found = @()
foreach ($name in $oldButtons) {
    $p = Join-Path $desktop $name
    if (Test-Path -LiteralPath $p) {
        $found += $p
        Write-Host "    移动  $name"
    }
    else {
        Write-Host "    跳过  $name（桌面上没有）" -ForegroundColor DarkGray
    }
}

if ($found.Count -eq 0) {
    Write-Host '    没有找到任何旧按钮，无需处理。' -ForegroundColor Green
    return
}

Write-Host ''
Write-Host "    共 $($found.Count) 个。它们是移动不是删除，随时可以从备份文件夹拖回来。" -ForegroundColor Yellow

if ($Apply) {
    New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
    foreach ($p in $found) {
        Move-Item -LiteralPath $p -Destination $backupDir -Force
    }
    Write-Host "    已移动 $($found.Count) 个到 $backupDir" -ForegroundColor Green
    Write-Host ''
    Write-Host '    提醒：确认面板里 7 项功能都正常之后，这个备份文件夹才可以删。' -ForegroundColor Yellow
}
