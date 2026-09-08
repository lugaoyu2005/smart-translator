# 窗口焦点/显隐行为检查（交付前必跑；需应用正在运行）
# 用法: powershell -File scripts/window-focus-check.ps1
# 方法来源: BUG报告-截图翻译窗口风暴.md（EnumWindows 采样 + GetForegroundWindow）

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class FocusCheck {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lp);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lp);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder sb, int max);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    public static bool ShotVisible = false;
    public static bool ForeIs(uint pid) {
        var fg = GetForegroundWindow();
        uint fpid; GetWindowThreadProcessId(fg, out fpid);
        return fpid == pid;
    }
    public static void Scan(uint pid) {
        ShotVisible = false;
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p == pid && IsWindowVisible(h)) {
                var sb = new StringBuilder(64);
                GetWindowText(h, sb, 64);
                // 精确匹配截图窗口标题（排除 13x13 内部小窗口误报）
                if (sb.ToString().Contains("\u622a\u56fe")) ShotVisible = true;
            }
            return true;
        }, IntPtr.Zero);
    }
}
"@

$ErrorActionPreference = "Stop"
$proc = Get-Process smart-translator -ErrorAction SilentlyContinue
if (-not $proc) { Write-Output "FAIL: 应用未运行"; exit 1 }
$pid0 = [uint32]($proc.Id | Select-Object -First 1)
$wsh = New-Object -ComObject WScript.Shell
$fail = 0

function Check($label, $expectVisible, $expectFocus) {
    [FocusCheck]::Scan($pid0)
    $vis = [FocusCheck]::ShotVisible
    $foc = [FocusCheck]::ForeIs($pid0)
    $ok = ($vis -eq $expectVisible) -and ($foc -eq $expectFocus)
    if (-not $ok) { $script:fail++ }
    Write-Output ("{0}: shot_visible={1} focus_ours={2} -> {3}" -f $label, $vis, $foc, $(if ($ok) { "PASS" } else { "FAIL" }))
}

# ===== 1. 待命态采样 45 秒：窗口零显隐 + 零前台抢占 =====
Write-Output "== 待命态采样 45 秒 =="
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$shotShown = 0; $foreTaken = 0; $n = 0
while ($sw.Elapsed.TotalSeconds -lt 45) {
    [FocusCheck]::Scan($pid0)
    if ([FocusCheck]::ShotVisible) { $shotShown++ }
    if ([FocusCheck]::ForeIs($pid0)) { $foreTaken++ }
    $n++
    Start-Sleep -Milliseconds 300
}
if ($shotShown -gt 0) { $fail++; Write-Output "采样${n}次: 截图窗口闪现 $shotShown 次 -> FAIL" }
else { Write-Output "采样${n}次: 截图窗口零闪现 -> PASS" }
if ($foreTaken -gt 0) { $fail++; Write-Output "前台被抢占 $foreTaken 次 -> FAIL" }
else { Write-Output "前台零抢占 -> PASS" }

# ===== 2. 触发链路 =====
Write-Output "== 触发链路 =="
$wsh.SendKeys("^%s")
Start-Sleep -Milliseconds 1500
Check "触发后（应显示+前台到手）" $true $true
$wsh.SendKeys("{ESC}")
Start-Sleep -Milliseconds 900
Check "ESC 后（应隐藏+释放前台）" $false $false
$wsh.SendKeys("^%s")
Start-Sleep -Milliseconds 1500
Check "重复触发" $true $true
$wsh.SendKeys("{ESC}")
Start-Sleep -Milliseconds 900
Check "清理" $false $false

if ($fail -eq 0) { Write-Output "== 全部通过 ==" } else { Write-Output "== $fail 项失败 =="; exit 1 }
