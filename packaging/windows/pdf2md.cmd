@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "bundle_dir=%~dp0"
set "PDFIUM_LIB_PATH=%bundle_dir%bin\pdfium.dll"
set "ORT_DYLIB_PATH=%bundle_dir%bin\onnxruntime.dll"
set "bundle_result=0"
set "staging_helper=%bundle_dir%bin\pdf2md-staging.exe"

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

rem Stage a short-named file beside the PDF so publishing stays on the same volume.
if not exist "%staging_helper%" goto helper_unavailable
set "reserve_attempt=0"

:reserve
set /a reserve_attempt+=1 >nul
set "staging=%~dp1.pdf2md-%RANDOM%-%RANDOM%-%RANDOM%.tmp"
"%staging_helper%" reserve "%staging%" 2>nul
if not errorlevel 0 goto helper_unavailable
if errorlevel 22 goto helper_unavailable
if errorlevel 21 goto failed
if errorlevel 20 if %reserve_attempt% LSS 3 goto reserve
if errorlevel 20 goto failed
if errorlevel 1 goto helper_unavailable

echo Conversione di "%input%" in corso...
"%bundle_dir%bin\pdf2md.exe" --ocr auto --ocr-offline --ocr-model-dir "%bundle_dir%models\pp-ocrv6-small" --raw --pages "%input%" > "%staging%"
if not errorlevel 0 goto cleanup_failed
if errorlevel 1 goto cleanup_failed
if not exist "%staging%" goto cleanup_failed

set "staging_without_text="
"%staging_helper%" has-text "%staging%" 2>nul
if not errorlevel 0 goto staging_helper_failed
if errorlevel 23 goto staging_helper_failed
if errorlevel 22 goto staging_empty
if errorlevel 21 goto staging_read_failed
if errorlevel 1 goto staging_helper_failed
goto publish

:staging_empty
set "staging_without_text=1"

:publish
set "overwrite="
if exist "%output%" set "overwrite=1"
set "publish_retries=0"

:publish_move
move /y "%staging%" "%output%" >nul 2>&1 || goto retry_publish
if defined overwrite (echo Sovrascritto: "%output%") else (echo Scritto: "%output%")
if defined staging_without_text echo Nessun testo trovato nel PDF.
goto advance

:retry_publish
if %publish_retries% GEQ 3 goto publish_failed
set /a publish_retries+=1 >nul
"%staging_helper%" wait-to-retry >nul 2>&1
if not errorlevel 0 goto staging_helper_failed
if errorlevel 1 goto staging_helper_failed
goto publish_move

:publish_failed
if exist "%staging%" del /q "%staging%" >nul 2>&1
"%staging_helper%" report-replace-failure "%output%"
goto mark_failed

:staging_read_failed
if exist "%staging%" del /q "%staging%" 2>nul
>&2 echo Errore: impossibile leggere il Markdown temporaneo di "%input%".
goto mark_failed

:staging_helper_failed
if exist "%staging%" del /q "%staging%" 2>nul
goto helper_unavailable

:helper_unavailable
>&2 echo Errore: impossibile avviare il componente di appoggio. Verifica che lo ZIP sia estratto per intero e che il componente non sia bloccato.
goto mark_failed

:cleanup_failed
if exist "%staging%" del /q "%staging%"

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
