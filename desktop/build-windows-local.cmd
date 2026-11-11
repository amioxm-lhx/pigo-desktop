@echo off
chcp 65001 >nul
title pigo-desktop Windows build
echo.
echo   pigo-desktop - Windows build
echo   ------------------------------------------------------------
echo   Run this from a NORMAL CMD/PowerShell window (not the agent session).
echo   If 360 / Defender prompts, choose "Allow" and add the target dir to the whitelist.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build-windows-local.ps1" %*
echo.
echo   Log: %~dp0build-windows-local.log
echo.
pause
