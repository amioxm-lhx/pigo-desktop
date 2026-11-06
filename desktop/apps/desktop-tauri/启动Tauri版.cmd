@echo off
rem pigo-desktop - Tauri 版本启动器
rem 前置条件：先关闭 Electron 版（两者共用 ~/.pi-agent-go-desktop 的 sqlite，避免争锁）
tasklist | findstr /i "pigo-desktop electron" >nul 2>&1
if %errorlevel%==0 (
  echo [提示] 检测到可能在运行的桌面实例，请先关闭 Electron 版再启动 Tauri 版。
  echo        仍要继续请直接双击 exe。
  pause
)
start "" "%~dp0pigo-desktop.exe"
