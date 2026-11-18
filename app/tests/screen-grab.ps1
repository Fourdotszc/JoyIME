# 截屏幕上的一块区域存成 PNG（物理像素坐标），用来看悬浮窗贴在光标旁边的样子。
# 用法：powershell -File app/tests/screen-grab.ps1 <输出.png> <x> <y> <宽> <高> [延迟秒数]
param([string]$Out, [int]$X, [int]$Y, [int]$W, [int]$H, [double]$Delay = 0)

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System.Runtime.InteropServices;
public static class Dpi { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }
'@
[Dpi]::SetProcessDPIAware() | Out-Null
Start-Sleep -Milliseconds ([int]($Delay * 1000))
$bmp = New-Object Drawing.Bitmap $W, $H
$g = [Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($X, $Y, 0, 0, $bmp.Size)
$bmp.Save($Out, [Drawing.Imaging.ImageFormat]::Png)
