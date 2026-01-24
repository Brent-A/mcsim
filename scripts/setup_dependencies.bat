@echo off
REM Quick setup script for MCSim dependencies
REM Run this to download all required external dependencies

echo MCSim Dependency Setup
echo ======================
echo.

REM Check for PowerShell and run the setup script
powershell -ExecutionPolicy Bypass -File "%~dp0setup_dependencies.ps1" %*

pause
