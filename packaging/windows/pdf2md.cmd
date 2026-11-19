@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "bundle_dir=%~dp0"
set "PDFIUM_LIB_PATH=%bundle_dir%bin\pdfium.dll"
set "ORT_DYLIB_PATH=%bundle_dir%bin\onnxruntime.dll"
set "bundle_result=0"

if not "%~1"=="" goto next
echo Trascina uno o piu PDF su pdf2md oppure scrivi pdf2md "nome.pdf".
set "bundle_result=1"
goto done

:next
if "%~1"=="" goto done
set "input=%~f1"
set "output=%~dpn1.md"
if /i not "%~x1"==".pdf" goto invalid
if not exist "%input%" goto invalid
set "input_attrs=%~a1"
if /i "%input_attrs:~0,1%"=="d" goto invalid
set "output_attrs="
for %%F in ("%output%") do set "output_attrs=%%~aF"
if /i "%output_attrs:~0,1%"=="d" goto failed

rem Stage the result beside the PDF so a conversion error preserves any old Markdown.
set "staging=%~dp1.pdf2md-%RANDOM%-%RANDOM%"
md "%staging%" 2>nul
if errorlevel 1 goto failed

echo Conversione di "%input%" in corso...
"%bundle_dir%bin\pdf2md.exe" --ocr auto --ocr-offline --ocr-model-dir "%bundle_dir%models\pp-ocrv6-small" --raw "%input%" > "%staging%\document.md"
if not errorlevel 0 goto cleanup_failed
if errorlevel 1 goto cleanup_failed
if not exist "%staging%\document.md" goto cleanup_failed

set "overwrite="
if exist "%output%" set "overwrite=1"
move /y "%staging%\document.md" "%output%" >nul
if errorlevel 1 goto cleanup_failed
rd "%staging%"
if defined overwrite (echo Sovrascritto: "%output%") else (echo Scritto: "%output%")
for %%F in ("%output%") do if %%~zF EQU 0 echo Nessun testo trovato nel PDF.
goto advance

:cleanup_failed
if exist "%staging%\document.md" del /q "%staging%\document.md"
rd "%staging%"

:failed
>&2 echo Errore: conversione di "%input%" non riuscita. Controlla il PDF e i permessi della cartella.
goto mark_failed

:invalid
>&2 echo Errore: "%input%" non esiste oppure non ha estensione .pdf.

:mark_failed
set "bundle_result=1"

:advance
shift
goto next

:done
call "%bundle_dir%bin\pausa-se-da-esplora.cmd"
exit /b %bundle_result%
