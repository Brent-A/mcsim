@echo off
REM Quick release builder for MCSim
REM Run this to build and package MCSim for distribution

echo MCSim Release Builder
echo =====================
echo.

REM Check for PowerShell and run the release script
powershell -ExecutionPolicy Bypass -File "%~dp0create_release.ps1" %*

pause
