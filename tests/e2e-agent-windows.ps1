# End-to-end test of the Ctrl+V agent on a real, interactive Windows desktop.
# NOT for CI (runners have no Explorer shell). It takes focus and injects key
# presses for about a minute: keep hands off the keyboard and mouse.
#
#   powershell -NoProfile -STA -File tests\e2e-agent-windows.ps1 -Bin target\release
#
# Leaves clipsave installed with the agent running. Restores clipboard text.
param(
    [Parameter(Mandatory)] [string] $Bin,
    [string] $Work = (Join-Path $env:TEMP 'clipsave-agent-e2e')
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class U {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int l, t, r, b; }
    [StructLayout(LayoutKind.Sequential)] public struct GUITHREADINFO {
        public int cbSize, flags; public IntPtr active, focus, capture, menuOwner, moveSize, caret; public RECT rc; }
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr pid);
    [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint tid, ref GUITHREADINFO gti);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    public static string Cls(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
    public static string Focus() {
        var fg = GetForegroundWindow(); var g = new GUITHREADINFO(); g.cbSize = Marshal.SizeOf(g);
        GetGUIThreadInfo(GetWindowThreadProcessId(fg, IntPtr.Zero), ref g);
        return Cls(fg) + " > " + Cls(GetAncestor(g.focus, 1)) + " > " + Cls(g.focus);
    }
    const uint UP = 2;
    public static void Key(byte vk, bool down) { keybd_event(vk, 0, down ? 0 : UP, UIntPtr.Zero); }
    public static void CtrlV(int repeats) {
        Key(0x11, true); for (int i = 0; i < repeats; i++) Key(0x56, true); Key(0x56, false); Key(0x11, false);
    }
    public static void Press(byte vk) { Key(vk, true); Key(vk, false); }
    public static void Ctrl(byte vk) { Key(0x11, true); Press(vk); Key(0x11, false); }
}
'@

function Check($cond, $what) { if ($cond) { "PASS  $what" } else { "FAIL  $what"; $script:failed = $true } }
function Pngs { @(Get-ChildItem $dir -Filter *.png -ErrorAction SilentlyContinue) }
function WaitPngs($n) { for ($i = 0; $i -lt 25 -and (Pngs).Count -lt $n; $i++) { Start-Sleep -Milliseconds 200 }; (Pngs).Count }
function SetPng {
    $bmp = New-Object System.Drawing.Bitmap 16, 16
    $ms = New-Object System.IO.MemoryStream; $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $data = New-Object System.Windows.Forms.DataObject; $data.SetData('PNG', $false, $ms)
    [System.Windows.Forms.Clipboard]::SetDataObject($data, $true)
}
function Focus-Explorer {
    [void][U]::SetForegroundWindow([IntPtr]$win.HWND); Start-Sleep -Milliseconds 400
    [U]::Press(0x1B)  # Esc: leave any edit box, focus stays on the item view
    Start-Sleep -Milliseconds 300
}

$bin = (Resolve-Path $Bin).Path
$dir = Join-Path $Work 'ไทย folder'
$src = Join-Path $Work 'source'
Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $dir, $src | Out-Null
Set-Content (Join-Path $src 'copied.txt') 'hello'
$backup = [System.Windows.Forms.Clipboard]::GetText()

& "$bin\clipsave.exe" install | Out-Null
Check ($LASTEXITCODE -eq 0) "install (starts the agent)"
Start-Sleep -Seconds 1
Check ([bool](Get-Process clipsavew -ErrorAction SilentlyContinue)) "agent process is running"
$runValue = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run').clipsave
Check ($runValue -like '*clipsavew.exe" agent') "Run key: $runValue"

$sh = New-Object -ComObject Shell.Application
function DirWindows { @($sh.Windows() | Where-Object { try { $_.Document.Folder.Self.Path -eq $dir } catch { $false } }) }
DirWindows | ForEach-Object { $_.Quit() }
$sh.Open($dir); Start-Sleep -Seconds 2
$win = DirWindows | Select-Object -First 1
Focus-Explorer
"INFO  focus in file list: $([U]::Focus())"

# 1. image -> exactly one PNG, selected
SetPng; Focus-Explorer
[U]::CtrlV(1)
Check ((WaitPngs 1) -eq 1) "Ctrl+V with an image saves one PNG"
Start-Sleep -Milliseconds 800
$sel = @(DirWindows | ForEach-Object { $_.Document.SelectedItems() } | ForEach-Object Name)
Check ($sel.Count -ge 1 -and ((Pngs)[0].BaseName -in $sel -or (Pngs)[0].Name -in $sel)) "new PNG is selected ($($sel -join ', '))"
"INFO  focus after save (rename box expected): $([U]::Focus())"

# 2. Ctrl+V held (auto-repeat) -> one more file, not a burst
Focus-Explorer
[U]::CtrlV(8)
Start-Sleep -Seconds 2
Check ((Pngs).Count -eq 2) "held Ctrl+V saves exactly one more PNG (have $((Pngs).Count))"

# 3. rename box focused -> nothing
Focus-Explorer
[U]::Press(0x71)  # F2 on the selected file
Start-Sleep -Milliseconds 500
"INFO  focus in rename box: $([U]::Focus())"
[U]::CtrlV(1); Start-Sleep -Seconds 2
Check ((Pngs).Count -eq 2) "Ctrl+V in the rename box saves nothing"
[U]::Press(0x1B)

# 4. copied file -> Explorer's own paste, no PNG
$files = New-Object System.Collections.Specialized.StringCollection
[void]$files.Add((Join-Path $src 'copied.txt'))
[System.Windows.Forms.Clipboard]::SetFileDropList($files)
Focus-Explorer
[U]::CtrlV(1)
for ($i = 0; $i -lt 25 -and -not (Test-Path (Join-Path $dir 'copied.txt')); $i++) { Start-Sleep -Milliseconds 200 }
Check (Test-Path (Join-Path $dir 'copied.txt')) "copied file: Explorer pastes it as usual"
Check ((Pngs).Count -eq 2) "copied file: no PNG saved"

# 5. text -> nothing
[System.Windows.Forms.Clipboard]::SetText('hello')
Focus-Explorer
[U]::CtrlV(1); Start-Sleep -Seconds 2
Check ((Pngs).Count -eq 2) "text clipboard: nothing saved"

# 6. tabs: a Home tab (virtual folder) saves nothing; a second folder tab
#    saves into ITS folder, not the first tab's
SetPng; Focus-Explorer
"INFO  focus in file list: $([U]::Focus())"
[U]::Ctrl(0x54)  # Ctrl+T -> Home
Start-Sleep -Seconds 2
"INFO  focus in Home tab: $([U]::Focus())"
[U]::CtrlV(1); Start-Sleep -Seconds 2
Check ((Pngs).Count -eq 2) "Ctrl+V in a Home tab saves nothing"
[U]::Ctrl(0x4C)  # Ctrl+L -> address bar, go to the source folder in this tab
Start-Sleep -Milliseconds 500
[System.Windows.Forms.SendKeys]::SendWait($src + '{ENTER}')
Start-Sleep -Seconds 2
[U]::Press(0x1B); Start-Sleep -Milliseconds 300
"INFO  focus in second folder tab: $([U]::Focus())"
[U]::CtrlV(1)
for ($i = 0; $i -lt 25 -and @(Get-ChildItem $src -Filter *.png).Count -lt 1; $i++) { Start-Sleep -Milliseconds 200 }
Check (@(Get-ChildItem $src -Filter *.png).Count -eq 1) "second tab: saved into its own folder"
Check ((Pngs).Count -eq 2) "second tab: nothing saved into the first tab's folder"
[U]::Press(0x1B)  # Esc: close the rename box, or Ctrl+W goes to it
Start-Sleep -Milliseconds 300
[U]::Ctrl(0x57)  # Ctrl+W closes the second tab
Start-Sleep -Seconds 1
Focus-Explorer
"INFO  focus back in first tab: $([U]::Focus())"
[U]::CtrlV(1)
Check ((WaitPngs 3) -eq 3) "back in the first tab: saves there"
Check (@(Get-ChildItem $src -Filter *.png).Count -eq 1) "back in the first tab: nothing saved into the closed tab's folder"

# 7. agent stopped -> Ctrl+V does nothing
& "$bin\clipsave.exe" agent stop | Out-Null
Check (-not (Get-Process clipsavew -ErrorAction SilentlyContinue)) "agent stop: process exited"
Focus-Explorer
[U]::CtrlV(1); Start-Sleep -Seconds 2
Check ((Pngs).Count -eq 3) "after agent stop: Ctrl+V saves nothing"

# leave it installed and running
& "$bin\clipsave.exe" install | Out-Null
Start-Sleep -Seconds 1
Check ([bool](Get-Process clipsavew -ErrorAction SilentlyContinue)) "reinstall restarts the agent"

DirWindows | ForEach-Object { $_.Quit() }
if ($backup) { [System.Windows.Forms.Clipboard]::SetText($backup) } else { [System.Windows.Forms.Clipboard]::Clear() }
if ($failed) { exit 1 }
