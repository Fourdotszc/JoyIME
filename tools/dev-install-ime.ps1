# 开发用：把 JoyIME 注册成这台电脑的系统输入法（需要管理员权限，会弹 UAC 确认）。
#   powershell -ExecutionPolicy Bypass -File tools\dev-install-ime.ps1            注册（先 cargo build 好两个 DLL）
#   powershell -ExecutionPolicy Bypass -File tools\dev-install-ime.ps1 -Uninstall 反注册
#
# 做的事：
# 1. 把 64 位和 32 位的 joyime_tsf.dll 拷到 C:\ProgramData\JoyIME-dev\x64 和 x86（不直接注册 target 目录，
#    不然 DLL 被程序加载着的时候重新编译会写不进去）
# 2. 给商店应用（新版记事本这类）读权限，不然它们加载不了 DLL
# 3. regsvr32 注册两个 DLL
# 4. 在 HKCU\Software\JoyIME 里写 ServicePath = 开发版后台程序，TsfLog = 调试日志路径
param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$dest = 'C:\ProgramData\JoyIME-dev'
$x64 = Join-Path $dest 'x64\joyime_tsf.dll'
$x86 = Join-Path $dest 'x86\joyime_tsf.dll'

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    $args = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`""
    if ($Uninstall) { $args += ' -Uninstall' }
    Start-Process powershell -Verb RunAs -ArgumentList $args -Wait
    exit
}

function Regsvr($exe, $dll, $extra) {
    $p = Start-Process $exe -ArgumentList "/s $extra `"$dll`"" -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "$exe $extra $dll 失败，退出码 $($p.ExitCode)" }
}

if ($Uninstall) {
    if (Test-Path $x64) { Regsvr 'C:\Windows\System32\regsvr32.exe' $x64 '/u' }
    if (Test-Path $x86) { Regsvr 'C:\Windows\SysWOW64\regsvr32.exe' $x86 '/u' }
    Remove-Item HKCU:\Software\JoyIME -Recurse -ErrorAction SilentlyContinue
    Write-Host '已反注册。已经打开的程序里如果还加载着 DLL，重启那些程序后才能删除目录。'
    Remove-Item $dest -Recurse -ErrorAction SilentlyContinue
    exit
}

New-Item -ItemType Directory -Force (Split-Path $x64), (Split-Path $x86) | Out-Null
Copy-Item (Join-Path $root 'target\release\joyime_tsf.dll') $x64 -Force
Copy-Item (Join-Path $root 'target\i686-pc-windows-msvc\release\joyime_tsf.dll') $x86 -Force
$ico = Join-Path $root 'app\src-tauri\icons\icon.ico'
Copy-Item $ico (Join-Path $dest 'x64\joyime.ico') -Force
Copy-Item $ico (Join-Path $dest 'x86\joyime.ico') -Force

# S-1-15-2-1 = ALL APPLICATION PACKAGES，S-1-15-2-2 = ALL RESTRICTED APPLICATION PACKAGES
icacls $dest /grant '*S-1-15-2-1:(OI)(CI)(RX)' '*S-1-15-2-2:(OI)(CI)(RX)' /T /Q | Out-Null

Regsvr 'C:\Windows\System32\regsvr32.exe' $x64 ''
Regsvr 'C:\Windows\SysWOW64\regsvr32.exe' $x86 ''

New-Item -Force HKCU:\Software\JoyIME | Out-Null
Set-ItemProperty HKCU:\Software\JoyIME ServicePath (Join-Path $root 'target\debug\joyime-app.exe')
Set-ItemProperty HKCU:\Software\JoyIME TsfLog (Join-Path $env:TEMP 'joyime-tsf.log')
Write-Host '已注册。在 设置 → 时间和语言 → 语言和区域 → 中文 → 键盘 里能看到「JoyIME 摇杆输入法」，Win + 空格 切换。'
