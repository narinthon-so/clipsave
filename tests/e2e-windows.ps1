# End-to-end test on real Windows: clipboard -> clipsave -> PNG on disk, plus
# `install` and the registry verbs run exactly as Explorer runs them.
# Overwrites the clipboard (text is restored) and installs clipsave for the current user.
#
#   powershell -NoProfile -STA -File tests\e2e-windows.ps1 -Bin target\release
param(
    [Parameter(Mandatory)] [string] $Bin,
    [string] $Work = (Join-Path $env:TEMP 'clipsave-e2e')
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$bin = (Resolve-Path $Bin).Path
$root = $Work
$dir = Join-Path $root 'ไทย folder'
Remove-Item -Recurse -Force $root\* -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $dir | Out-Null
$backup = [System.Windows.Forms.Clipboard]::GetText()

function Check($cond, $what) { if ($cond) { "PASS  $what" } else { "FAIL  $what"; $script:failed = $true } }
function Pixel($path, $x, $y) { $b = [System.Drawing.Bitmap]::FromFile($path); try { $b.GetPixel($x, $y) } finally { $b.Dispose() } }

# 64x32 image: left half opaque red, right half fully transparent, (40,5) half-transparent blue
$bmp = New-Object System.Drawing.Bitmap 64, 32, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
for ($x = 0; $x -lt 32; $x++) { for ($y = 0; $y -lt 32; $y++) { $bmp.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, 255, 0, 0)) } }
$bmp.SetPixel(40, 5, [System.Drawing.Color]::FromArgb(128, 0, 0, 255))
$ms = New-Object System.IO.MemoryStream; $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)

# 1. Figma-style: registered "PNG" format on the clipboard
$data = New-Object System.Windows.Forms.DataObject
$data.SetData('PNG', $false, $ms)
[System.Windows.Forms.Clipboard]::SetDataObject($data, $true)
$out = & "$bin\clipsave.exe" save $dir --name 'หน้าแรก'
Check ($LASTEXITCODE -eq 0) "PNG format: exit 0"
Check ($out -eq (Join-Path $dir 'หน้าแรก.png')) "PNG format: prints path ($out)"
$p = Pixel $out 0 0; Check ($p.A -eq 255 -and $p.R -eq 255) "opaque pixel kept"
$p = Pixel $out 63 31; Check ($p.A -eq 0) "transparent pixel kept"
$p = Pixel $out 40 5; Check ($p.A -eq 128 -and $p.B -eq 255) "half-alpha pixel kept (A=$($p.A))"

# 2. Collision -> suffix, original untouched
$out2 = & "$bin\clipsave.exe" $dir -n 'หน้าแรก.png'
Check ($out2 -eq (Join-Path $dir 'หน้าแรก-1.png')) "collision gets -1 suffix"

# 3. DIB only (screenshots, Paint): SetImage
[System.Windows.Forms.Clipboard]::SetImage($bmp)
$out3 = & "$bin\clipsave.exe" $dir
Check ($LASTEXITCODE -eq 0 -and (Split-Path $out3 -Leaf) -match '^image-\d{8}-\d{6}(-\d+)?\.png$') "DIB: default name ($(Split-Path $out3 -Leaf))"
$b = [System.Drawing.Bitmap]::FromFile($out3); Check ($b.Width -eq 64 -and $b.Height -eq 32) "DIB: size 64x32"; $b.Dispose()

# 4. Text only -> exit 1, nothing written
[System.Windows.Forms.Clipboard]::SetText('hello')
$before = (Get-ChildItem $dir).Count
$ErrorActionPreference = 'Continue'; & "$bin\clipsave.exe" $dir 2>$null; $code = $LASTEXITCODE; $ErrorActionPreference = 'Stop'
Check ($code -eq 1) "text clipboard: exit 1"
Check ((Get-ChildItem $dir).Count -eq $before) "text clipboard: no file written"

# 5. Install, then run the registry command exactly like Explorer would
[System.Windows.Forms.Clipboard]::SetDataObject($data, $true)
& "$bin\clipsave.exe" install | Out-Null
Check ($LASTEXITCODE -eq 0) "install exit 0"
$cmd = (Get-ItemProperty 'HKCU:\Software\Classes\Directory\shell\clipsave\command').'(default)'
Check ($cmd -like '*\Programs\clipsave\clipsavew.exe" save --gui "%1"') "folder verb command: $cmd"
$bgcmd = (Get-ItemProperty 'HKCU:\Software\Classes\Directory\Background\shell\clipsave\command').'(default)'
Check ($bgcmd -like '*clipsavew.exe" save --gui --reveal "%V"') "background verb command: $bgcmd"

function RunVerb($line, $target) {
    $line = $line.Replace('%1', $target).Replace('%V', $target)
    $exe = $line.Substring(1, $line.IndexOf('"', 1) - 1)
    $args = $line.Substring($line.IndexOf('"', 1) + 2)
    $p = [System.Diagnostics.Process]::Start($exe, $args); $p.WaitForExit(10000) | Out-Null; $p.ExitCode
}
$before = (Get-ChildItem $dir).Count
Check ((RunVerb $cmd $dir) -eq 0) "folder verb on Thai+space folder: exit 0"
Check ((Get-ChildItem $dir).Count -eq $before + 1) "folder verb wrote a file"

# 6. Drive root: "%V" -> "X:\" arrives as X:"
$drive = (69..90 | ForEach-Object { [char]$_ + ':' } | Where-Object { -not (Test-Path "$_\") } | Select-Object -Last 1)
subst $drive $dir
try {
    $before = (Get-ChildItem $dir).Count
    Check ((RunVerb $cmd "$drive\") -eq 0) "drive root ($drive\): exit 0"
    Check ((Get-ChildItem $dir).Count -eq $before + 1) "drive root wrote a file"
} finally { subst $drive /D }

Get-ChildItem $dir | ForEach-Object { "      $($_.Name)  $($_.Length) bytes" }
if ($backup) { [System.Windows.Forms.Clipboard]::SetText($backup) } else { [System.Windows.Forms.Clipboard]::Clear() }
if ($failed) { exit 1 }
