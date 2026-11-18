# 端到端测试用的「接收窗口」：一个带文本框的小窗口，拿到焦点后等 JoyIME 打字，
# 到时间把文本框内容和「文本框是否一直有焦点」写进文件，然后自己关掉。
# 用法：powershell -File app/tests/e2e-textbox.ps1 <输出文件> <等待秒数>
param([string]$Out, [int]$Seconds = 20)

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$form = New-Object Windows.Forms.Form
$form.Text = 'JoyIME 端到端测试'
$form.TopMost = $true
$form.Width = 700
$form.Height = 220
$form.StartPosition = 'CenterScreen'
$box = New-Object Windows.Forms.TextBox
$box.Multiline = $true
$box.Dock = 'Fill'
$box.Font = New-Object Drawing.Font('Microsoft YaHei', 16)
$form.Controls.Add($box)

$lostFocus = 0
$box.Add_LostFocus({ $script:lostFocus++ })

$timer = New-Object Windows.Forms.Timer
$timer.Interval = $Seconds * 1000
$timer.Add_Tick({
    $timer.Stop()
    [IO.File]::WriteAllText($Out, "TEXT=" + $box.Text + "`nLOST_FOCUS=" + $script:lostFocus, [Text.Encoding]::UTF8)
    $form.Close()
})
$form.Add_Shown({ $form.Activate(); $box.Focus(); $timer.Start() })
[Windows.Forms.Application]::Run($form)
