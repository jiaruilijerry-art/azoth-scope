# Run after `cargo build --locked --example observe_oled`. Diagnostic only;
# does not open the device for writing, switch modes, or change app settings.
param([ValidateRange(1,15)][int]$Minutes=10)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $root 'target\debug\examples\observe_oled.exe'
if (!(Test-Path -LiteralPath $exe)) { throw 'Build observe_oled first.' }
$base=Join-Path (Join-Path $root 'target') ("oled-event-"+(Get-Date -Format 'yyyyMMdd-HHmmss'))
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
$form=New-Object Windows.Forms.Form
$form.Text='Azoth Scope - Flash marker';$form.TopMost=$true
$form.FormBorderStyle='FixedToolWindow';$form.StartPosition='Manual'
$form.ClientSize=New-Object Drawing.Size 288,92
$area=[Windows.Forms.Screen]::PrimaryScreen.WorkingArea
$form.Location=New-Object Drawing.Point ($area.Right-$form.Width-12),($area.Bottom-$form.Height-12)
$button=New-Object Windows.Forms.Button;$button.Text='Saw the wallpaper';$button.SetBounds(10,8,268,40);$form.Controls.Add($button)
$label=New-Object Windows.Forms.Label;$label.SetBounds(10,54,268,32);$form.Controls.Add($label)
$end=(Get-Date).AddMinutes($Minutes);$script:count=0
$button.Add_Click({$script:count++; [IO.File]::AppendAllText("$base.marks",[DateTime]::UtcNow.ToString('o')+" saw wallpaper`r`n");$button.Text="Marked ($script:count) - mark next flash"})
$observer=Start-Process -FilePath $exe -ArgumentList '--timeline',($Minutes*60) -PassThru -WindowStyle Hidden -RedirectStandardOutput "$base.log" -RedirectStandardError "$base.err"
$timer=New-Object Windows.Forms.Timer;$timer.Interval=500
$timer.Add_Tick({
  $left=[Math]::Ceiling(($end-(Get-Date)).TotalSeconds)
  if ($observer.HasExited -or $left -le 0) { $form.Close(); return }
  $label.Text="Read-only USB trace: $left s left. Close to stop."
})
try {$timer.Start();[void]$form.ShowDialog()}finally{$timer.Stop();$timer.Dispose();$form.Dispose();if(!$observer.HasExited){$observer.Kill()};$observer.Dispose()}
