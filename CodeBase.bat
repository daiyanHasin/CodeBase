@echo off
setlocal
title CodeBase
cd /d "%~dp0"

rem If Rust is installed, rebuild first. This takes a few minutes the very first time,
rem then about a second, and it guarantees you always run the newest files in this folder.
where cargo >nul 2>nul
if not errorlevel 1 (
  if not exist "target\release\codebase.exe" echo First run: building CodeBase. This takes a few minutes, only once.
  cargo build --release
  if errorlevel 1 (
    echo.
    echo The build failed. Read the messages above, or see Troubleshooting in README.md.
    pause
    exit /b 1
  )
  "target\release\codebase.exe" %*
  if errorlevel 1 pause
  exit /b
)

rem No Rust: use a ready-made program placed next to this file.
if exist "codebase.exe" (
  "codebase.exe" %*
  if errorlevel 1 pause
  exit /b
)

echo.
echo CodeBase is not built yet.
echo.
echo   Option 1: put a ready-made codebase.exe in this folder, then run this file again.
echo   Option 2: install Rust from https://rustup.rs , then run this file again.
echo.
echo See README.md for details.
echo.
pause
exit /b 1
