@echo off
title ScreenMate Launcher

echo ===================================================
echo   ScreenMate - Desktop AI Companion Launcher
echo ===================================================
echo.
echo   [1/2] Moving to backend directory...
cd /d "%~dp0src-tauri"
echo   [2/2] Launching ScreenMate in development mode...
echo.
echo   - To close the app: Right-click character -^> Exit,
echo     or close the taskbar window / press Ctrl+C here.
echo ===================================================
echo.

cargo run
