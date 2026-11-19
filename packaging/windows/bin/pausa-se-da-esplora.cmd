@echo off
setlocal EnableExtensions DisableDelayedExpansion
rem PowerShell can also start cmd /c. Check whether Explorer started that cmd.
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -Command ^
  "$ErrorActionPreference = 'Stop';" ^
  "$parent = (Get-CimInstance Win32_Process -Filter ('ProcessId=' + $PID) -OperationTimeoutSec 5).ParentProcessId;" ^
  "$shell = Get-CimInstance Win32_Process -Filter ('ProcessId=' + $parent) -OperationTimeoutSec 5;" ^
  "if (!$shell.CommandLine) { exit 1 }" ^
  "if ($shell.CommandLine -match '(?i)\s/c\b' -and (Get-Process -Id $shell.ParentProcessId).ProcessName -eq 'explorer') { exit 10 }" ^
  "exit 0" 2>nul
if not "%errorlevel%"=="0" (
    echo Premi un tasto per chiudere.
    pause >nul
)
exit /b 0
