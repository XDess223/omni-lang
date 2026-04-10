@echo off
setlocal enabledelayedexpansion

echo ==========================================
echo    Omni VS Code Extension Refresher
echo ==========================================

:: 1. Configuration
set EXT_DIR=omni-vscode
set PUBLISHER=XDess223
set EXT_NAME=omni-lang
set VERSION=1.0.0
set VSIX_FILE=%EXT_NAME%-%VERSION%.vsix

echo [1/4] Checking for vsce...
where vsce >nul 2>nul
if %errorlevel% neq 0 (
    echo [!] vsce not found. Installing @vscode/vsce globally...
    call npm install -g @vscode/vsce
)

:: 2. Uninstall existing extension
echo [2/4] Uninstalling existing extension...
call code --uninstall-extension %PUBLISHER%.%EXT_NAME% >nul 2>nul
if %errorlevel% neq 0 (
    echo [i] Note: Clean uninstall skip - extension might not be present.
)

:: 3. Package the extension
echo [3/4] Packaging extension...
if not exist "%EXT_DIR%" (
    echo [!] Extension directory %EXT_DIR% not found.
    exit /b 1
)

pushd %EXT_DIR%
if exist "%VSIX_FILE%" del /q "%VSIX_FILE%"
call vsce package --no-dependencies
if %errorlevel% neq 0 (
    echo [!] Packaging failed.
    popd
    exit /b 1
)
popd

:: 4. Install the new extension
echo [4/4] Installing new extension...
if not exist "%EXT_DIR%\%VSIX_FILE%" (
    echo [!] Generated VSIX file %EXT_DIR%\%VSIX_FILE% not found.
    exit /b 1
)

call code --install-extension "%EXT_DIR%\%VSIX_FILE%"
if %errorlevel% neq 0 (
    echo [!] Installation failed.
    exit /b 1
)

echo.
echo SUCCESS! Please RESTART VS Code to activate changes.
pause
