@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "PDF2MD_BUNDLE_DIR=%~dp0."
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -Command ^
  "$ErrorActionPreference = 'Stop';" ^
  "if ($ExecutionContext.SessionState.LanguageMode -ne 'FullLanguage') {" ^
  "  Write-Host 'PowerShell in modalita limitata. Il PATH utente resta invariato. Puoi trascinare i PDF su pdf2md.'; exit 2" ^
  "}" ^
  "try {" ^
  "  $directory = [IO.Path]::GetFullPath($env:PDF2MD_BUNDLE_DIR);" ^
  "  if ($directory.Contains(';')) { throw 'Sposta pdf2md in una cartella senza punti e virgola nel percorso.' }" ^
  "  $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment');" ^
  "  try {" ^
  "    $path = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames');" ^
  "    $entries = ($path -split ';') | ForEach-Object { [Environment]::ExpandEnvironmentVariables($_.Trim().Trim([char]34)).TrimEnd('\', '/') };" ^
  "    if ($entries -notcontains $directory.TrimEnd('\', '/')) {" ^
  "      $kind = if ($key.GetValueNames() -contains 'Path') { $key.GetValueKind('Path') } else { 'ExpandString' };" ^
  "      $separator = if ($path -and !$path.EndsWith(';')) { ';' } else { '' };" ^
  "      $newPath = $path + $separator + $directory;" ^
  "      $key.SetValue('Path', $newPath, $kind);" ^
  "    }" ^
  "  } finally { $key.Dispose() }" ^
  "} catch { [Console]::Error.WriteLine('Errore durante l''aggiornamento del PATH utente. ' + $_.Exception.Message); exit 1 }" ^
  "try {" ^
  "  $signature = '[DllImport(' + [char]34 + 'user32.dll' + [char]34 + ', CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);';" ^
  "  Add-Type -Namespace Pdf2md -Name NativeMethods -MemberDefinition $signature;" ^
  "  $ignored = [UIntPtr]::Zero;" ^
  "  $notified = [Pdf2md.NativeMethods]::SendMessageTimeout([IntPtr]0xffff, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 2000, [ref]$ignored);" ^
  "  if ($notified -eq [IntPtr]::Zero) { throw 'Environment notification failed' }" ^
  "  Write-Host 'pdf2md disponibile. Chiudi il terminale e aprine uno nuovo.';" ^
  "} catch { Write-Host 'La cartella di pdf2md e nel PATH utente. Per applicarlo, esci da Windows e accedi di nuovo.' }" ^
  "exit 0"
set "path_result=%errorlevel%"
if "%path_result%"=="9009" >&2 echo PowerShell non disponibile. Il PATH utente resta invariato.
call "%~dp0bin\pausa-se-da-esplora.cmd"
exit /b %path_result%
