$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

$root = Join-Path $env:RUNNER_TEMP 'staging-watch'
$null = New-Item -ItemType Directory -Path $root
Copy-Item -LiteralPath $env:PDF2MD_EXTRACTED -Destination $root -Recurse
$bundle = Join-Path $root 'pdf2md'
$launcher = Join-Path $bundle 'pdf2md.cmd'
$legacyLauncher = Join-Path $bundle 'legacy.cmd'
$legacyTag = 'pdf2md-windows-1.26.1-1'
git fetch --no-tags --depth=1 origin "refs/tags/$legacyTag"
if ($LASTEXITCODE -ne 0) { throw "Could not fetch the previous release tag $legacyTag" }
$legacy = git show 'FETCH_HEAD:packaging/windows/pdf2md.cmd'
if ($LASTEXITCODE -ne 0) { throw "Could not read the launcher from $legacyTag" }
[IO.File]::WriteAllLines($legacyLauncher, $legacy, [Text.Encoding]::ASCII)

$source = Join-Path $root 'barrier.rs'
@'
use std::{env, fs, io::{self, Write}, path::{Path, PathBuf}, process::{Command, ExitCode}, thread, time::{Duration, Instant}};

fn wait_for_release(release: &Path) {
    let deadline = Instant::now() + Duration::from_secs(40);
    while !release.exists() {
        assert!(Instant::now() < deadline, "staging observer did not release the barrier");
        thread::sleep(Duration::from_millis(20));
    }
}

fn main() -> ExitCode {
    let executable = env::current_exe().unwrap();
    if executable.file_stem().unwrap() == "pdf2md-staging" {
        if env::args_os().nth(1).is_some_and(|arg| arg == "wait-to-retry") {
            // Reaching this point proves the first move failed. A second wait is a test failure.
            fs::File::create_new(executable.with_file_name("retry.ready")).unwrap();
            wait_for_release(&executable.with_file_name("retry.release"));
        }
        let status = Command::new(executable.with_file_name("pdf2md-staging-real.exe"))
            .args(env::args_os().skip(1))
            .status()
            .unwrap();
        return ExitCode::from(u8::try_from(status.code().unwrap()).unwrap());
    }

    let pdf = PathBuf::from(env::args_os().last().unwrap());
    fs::write(pdf.with_extension("ready"), "").unwrap();
    wait_for_release(&pdf.with_extension("release"));
    if pdf.file_stem().unwrap() == "illeggibile" {
        io::stdout().write_all(&[0xff]).unwrap();
    } else {
        print!("<!-- Page 1 -->\n\nMarkdown dal sostituto\n");
    }
    ExitCode::SUCCESS
}
'@ | Set-Content -LiteralPath $source -Encoding utf8
rustc --edition 2021 $source -o (Join-Path $bundle 'bin/pdf2md.exe')
if ($LASTEXITCODE -ne 0) { throw 'The barrier substitute did not compile' }

$signature = @'
[DllImport("kernel32.dll", ExactSpelling = true, CharSet = CharSet.Unicode, SetLastError = true)]
public static extern Microsoft.Win32.SafeHandles.SafeFileHandle CreateFileW(
    string name, uint access, uint share, IntPtr security, uint disposition,
    uint flags, IntPtr template);
'@
Add-Type -Namespace Pdf2mdTest -Name NativeFile -MemberDefinition $signature
$processes = [Collections.Generic.List[Diagnostics.Process]]::new()
$handles = [Collections.Generic.List[Microsoft.Win32.SafeHandles.SafeFileHandle]]::new()

function Start-Conversion([string]$script, [string]$pdf) {
    Copy-Item -LiteralPath tests/fixtures/thermo-freon12.pdf -Destination $pdf
    $stdout = Join-Path $root "$($processes.Count).stdout.txt"
    $stderr = Join-Path $root "$($processes.Count).stderr.txt"
    $arguments = "/d /c call `"$script`" `"$pdf`""
    $process = Start-Process -FilePath $env:ComSpec -ArgumentList $arguments -NoNewWindow -PassThru `
        -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $processes.Add($process)
    $null = $process.Handle
    [pscustomobject]@{
        Process = $process
        Ready = [IO.Path]::ChangeExtension($pdf, '.ready')
        Release = [IO.Path]::ChangeExtension($pdf, '.release')
        Output = [IO.Path]::ChangeExtension($pdf, '.md')
        Pdf = $pdf
        Stdout = $stdout
        Stderr = $stderr
    }
}

function Wait-ForBarrier($run, [string]$path = $run.Ready) {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    while (!(Test-Path -LiteralPath $path -PathType Leaf)) {
        if ($run.Process.HasExited -or [DateTime]::UtcNow -gt $deadline) {
            throw "Substitute did not reach barrier '$path': $(Get-Content -LiteralPath $run.Stderr -Raw)"
        }
        Start-Sleep -Milliseconds 20
    }
}

function Hold-Staging($entry, [uint32]$access = 0) {
    $share = [IO.FileShare]'ReadWrite, Delete'
    $flags = 0
    if ($entry.PSIsContainer) {
        # A directory handle with read access and no delete sharing blocks the old rd.
        $access = 0x80000000u
        $share = [IO.FileShare]::ReadWrite
        $flags = 0x02000000
    }
    $handle = [Pdf2mdTest.NativeFile]::CreateFileW(
        $entry.FullName, $access, [uint32]$share, [IntPtr]::Zero, 3, $flags, [IntPtr]::Zero)
    if ($handle.IsInvalid) {
        throw "Could not hold staging open: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $handles.Add($handle)
}

function Close-StagingHandles {
    foreach ($handle in $handles) { $handle.Dispose() }
    $handles.Clear()
}

function Wait-ForConversion($run, [int]$timeout) {
    if (!$run.Process.WaitForExit($timeout)) {
        throw "Launcher did not exit: $($run.Pdf)"
    }
    $run.Process.WaitForExit()
}

function Wait-ForSuccessfulConversion($run) {
    Wait-ForConversion $run 15000
    if ($run.Process.ExitCode -ne 0) {
        throw "Launcher did not complete publication: $(Get-Content -LiteralPath $run.Stderr -Raw)"
    }
    $markdown = Get-Content -LiteralPath $run.Output -Raw
    if ($markdown.Trim() -cne "<!-- Page 1 -->`n`nMarkdown dal sostituto") {
        throw "Expected Markdown from the barrier substitute: $markdown"
    }
}

$savedRandom = $env:RANDOM
$savedErrorLevel = $env:ERRORLEVEL
$shortDirectory = $null
try {
    # Keep the total path below MAX_PATH while exercising a 240-character filename.
    $positive = Join-Path ("$env:SystemDrive" + '\') "p$PID"
    $null = New-Item -ItemType Directory -Path $positive
    $shortDirectory = $positive
    $env:RANDOM = $null
    $longName = ('a' * 236) + '.pdf'
    $stagingNames = @()
    $runs = foreach ($name in $longName, 'secondo.pdf') {
        $run = Start-Conversion $launcher (Join-Path $positive $name)
        Wait-ForBarrier $run
        $entries = @(Get-ChildItem -LiteralPath $positive -Force -File -Filter '.pdf2md-*.tmp' |
            Where-Object Name -notin $stagingNames)
        if ($entries.Count -ne 1 -or $entries[0].Name -cnotmatch '^\.pdf2md-\d+-\d+-\d+\.tmp$') {
            throw "Expected a distinct short staging name for $name"
        }
        Hold-Staging $entries[0]
        $stagingNames += $entries[0].Name
        $run
    }
    foreach ($run in $runs) {
        [IO.File]::WriteAllText($run.Release, '')
        Wait-ForSuccessfulConversion $run
        $messages = @(Get-Content -LiteralPath $run.Stdout)
        if ($messages.Count -ne 2 -or $messages[0] -cne ("Conversione di " + '"' + $run.Pdf + '" in corso...') -or
            $messages[1] -cne ('Scritto: "' + $run.Output + '"')) {
            throw "Unexpected launcher messages: $messages"
        }
        if ((Get-Item -LiteralPath $run.Stderr).Length -ne 0) { throw "Unexpected stderr for $($run.Pdf)" }
    }
    # Renaming does not close the handles. Release them before replacing the resulting .md.
    Close-StagingHandles
    # Also convert the long-named PDF with the real extracted executable.
    $longRun = $runs[0]
    & "$env:PDF2MD_EXTRACTED/pdf2md.cmd" $longRun.Pdf
    if ($LASTEXITCODE -ne 0 -or (Get-Content -LiteralPath $longRun.Output -Raw) -notmatch 'Freon' -or
        @(Get-ChildItem -LiteralPath $positive -Force -Filter '.pdf2md-*').Count -ne 0) {
        throw 'The long-named PDF did not convert cleanly'
    }

    $lockedDirectory = Join-Path $env:PDF2MD_TEST_DIR 'destinazione-aperta'
    $null = New-Item -ItemType Directory -Path $lockedDirectory
    $previousMarkdown = [byte[]](0xef, 0xbb, 0xbf, 0x4d, 0x0d, 0x0a)
    $lockedPdf = Join-Path $lockedDirectory 'occupato.pdf'
    $lockedOutput = [IO.Path]::ChangeExtension($lockedPdf, '.md')
    [IO.File]::WriteAllBytes($lockedOutput, $previousMarkdown)
    Hold-Staging (Get-Item -LiteralPath $lockedOutput) 0x80000000u
    $locked = Start-Conversion $launcher $lockedPdf
    Wait-ForBarrier $locked
    [IO.File]::WriteAllText($locked.Release, '')
    Wait-ForConversion $locked 15000
    $diagnostics = @(Get-Content -LiteralPath $locked.Stderr -Encoding utf8)
    $expected = 'Errore: impossibile sostituire "{0}": il file è aperto in un altro programma. Chiudilo e riprova.' -f $locked.Output
    $messages = @(Get-Content -LiteralPath $locked.Stdout)
    if ($locked.Process.ExitCode -ne 1 -or $diagnostics.Count -ne 1 -or $diagnostics[0] -cne $expected -or
        $messages.Count -ne 1 -or $messages[0] -cne ('Conversione di "{0}" in corso...' -f $locked.Pdf) -or
        [Convert]::ToHexString([IO.File]::ReadAllBytes($locked.Output)) -cne 'EFBBBF4D0D0A' -or
        @(Get-ChildItem -LiteralPath $lockedDirectory -Force -Filter '.pdf2md-*').Count -ne 0) {
        throw "Expected the existing Markdown to survive a persistent lock: $diagnostics"
    }
    Close-StagingHandles

    # The proxy holds the first retry at a barrier, so releasing the handle needs no timing guess.
    $helper = Join-Path $bundle 'bin/pdf2md-staging.exe'
    $realHelper = Join-Path $bundle 'bin/pdf2md-staging-real.exe'
    $retryReady = Join-Path $bundle 'bin/retry.ready'
    $retryRelease = Join-Path $bundle 'bin/retry.release'
    Move-Item -LiteralPath $helper -Destination $realHelper
    Copy-Item -LiteralPath (Join-Path $bundle 'bin/pdf2md.exe') -Destination $helper
    $transientPdf = Join-Path $lockedDirectory 'rilasciato.pdf'
    $transientOutput = [IO.Path]::ChangeExtension($transientPdf, '.md')
    [IO.File]::WriteAllBytes($transientOutput, $previousMarkdown)
    Hold-Staging (Get-Item -LiteralPath $transientOutput) 0x80000000u
    $transient = Start-Conversion $launcher $transientPdf
    Wait-ForBarrier $transient
    [IO.File]::WriteAllText($transient.Release, '')
    Wait-ForBarrier $transient $retryReady
    Close-StagingHandles
    [IO.File]::WriteAllText($retryRelease, '')
    Wait-ForSuccessfulConversion $transient
    $messages = @(Get-Content -LiteralPath $transient.Stdout)
    if ($messages.Count -ne 2 -or $messages[1] -cne ('Sovrascritto: "{0}"' -f $transient.Output) -or
        (Get-Item -LiteralPath $transient.Stderr).Length -ne 0 -or
        @(Get-ChildItem -LiteralPath $lockedDirectory -Force -Filter '.pdf2md-*').Count -ne 0) {
        throw 'Expected a clean replacement at the second move'
    }
    Move-Item -LiteralPath $realHelper -Destination $helper -Force

    # Make every candidate name collide and mask the dynamic %ERRORLEVEL% expansion.
    $env:ERRORLEVEL = '0'
    $env:RANDOM = '12345'
    $conflict = Join-Path $root '.pdf2md-12345-12345-12345.tmp'
    [IO.File]::WriteAllBytes($conflict, [byte[]](0xef, 0xbb, 0xbf, 0x43, 0x0d, 0x0a))
    $blocked = Start-Conversion $launcher (Join-Path $root 'occupato.pdf')
    Wait-ForConversion $blocked 10000
    if ($blocked.Process.ExitCode -ne 1 -or
        (Test-Path -LiteralPath $blocked.Ready) -or (Get-Item -LiteralPath $blocked.Stdout).Length -ne 0 -or
        [Convert]::ToHexString([IO.File]::ReadAllBytes($conflict)) -cne 'EFBBBF430D0A' -or
        @(Get-Content -LiteralPath $blocked.Stderr).Count -ne 1 -or
        (Get-Content -LiteralPath $blocked.Stderr -Raw) -cnotmatch '^Errore: conversione di ') {
        throw 'Expected a quiet reservation failure without launching or deleting the other staging entry'
    }

    $env:ERRORLEVEL = $savedErrorLevel
    $negative = Join-Path $root 'legacy'
    $null = New-Item -ItemType Directory -Path $negative
    $run = Start-Conversion $legacyLauncher (Join-Path $negative 'precedente.pdf')
    Wait-ForBarrier $run
    $directory = Get-Item -LiteralPath (Join-Path $negative '.pdf2md-12345-12345')
    if (!$directory.PSIsContainer) { throw 'The old launcher must create a staging directory' }
    Hold-Staging $directory
    [IO.File]::WriteAllText($run.Release, '')
    Wait-ForSuccessfulConversion $run
    $diagnostics = @(Get-Content -LiteralPath $run.Stderr)
    $removalErrors = @(5, 32 | ForEach-Object { [ComponentModel.Win32Exception]::new($_).Message.Trim().TrimEnd('.') })
    if ($diagnostics.Count -ne 1 -or $diagnostics[0].Trim().TrimEnd('.') -notin $removalErrors -or
        !(Test-Path -LiteralPath $directory.FullName -PathType Container) -or
        @(Get-ChildItem -LiteralPath $directory.FullName -Force).Count -ne 0) {
        throw "Expected the old launcher to publish Markdown, report the removal failure and leave an empty directory: $diagnostics"
    }
    Write-Host "Previous release reproduced the cleanup defect: $diagnostics"

    $env:RANDOM = $null
    $env:ERRORLEVEL = '0'
    $unreadable = Start-Conversion $launcher (Join-Path $env:PDF2MD_TEST_DIR 'illeggibile.pdf')
    [IO.File]::WriteAllBytes($unreadable.Output, [byte[]](0xef, 0xbb, 0xbf, 0x4d, 0x0d, 0x0a))
    Wait-ForBarrier $unreadable
    [IO.File]::WriteAllText($unreadable.Release, '')
    Wait-ForConversion $unreadable 10000
    if ($unreadable.Process.ExitCode -ne 1 -or
        [Convert]::ToHexString([IO.File]::ReadAllBytes($unreadable.Output)) -cne 'EFBBBF4D0D0A' -or
        (Get-Content -LiteralPath $unreadable.Stdout -Raw) -match '(?m)^(?:Scritto|Sovrascritto):' -or
        (Get-Content -LiteralPath $unreadable.Stderr -Raw) -cnotmatch '^Errore: impossibile leggere il Markdown temporaneo') {
        throw 'A staging read error must prevent publication and preserve the previous Markdown'
    }

    $env:ERRORLEVEL = $savedErrorLevel
    Move-Item -LiteralPath $helper -Destination "$helper.saved"
    foreach ($condition in 'missing', 'unlaunchable') {
        if ($condition -eq 'unlaunchable') { [IO.File]::WriteAllText($helper, 'Not an executable') }
        $run = Start-Conversion $launcher (Join-Path $root "$condition.pdf")
        Wait-ForConversion $run 10000
        if ($run.Process.ExitCode -ne 1 -or (Test-Path -LiteralPath $run.Ready) -or
            (Get-Item -LiteralPath $run.Stdout).Length -ne 0 -or
            (Get-Content -LiteralPath $run.Stderr -Raw) -cnotmatch '^Errore: impossibile avviare il componente di appoggio') {
            throw "Expected the staging helper diagnostic when it is $condition"
        }
    }
} finally {
    $env:RANDOM = $savedRandom
    $env:ERRORLEVEL = $savedErrorLevel
    foreach ($process in $processes) {
        if (!$process.HasExited) { $process.Kill($true) }
        $process.Dispose()
    }
    Close-StagingHandles
    if ($shortDirectory) { Remove-Item -LiteralPath $shortDirectory -Recurse -Force }
}
