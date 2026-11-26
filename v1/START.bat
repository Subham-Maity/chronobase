@echo off
title ChronoBase — Backup Manager
color 0A
cls

echo.
echo  ╔═══════════════════════════════════════════╗
echo  ║   ChronoBase  ·  Database Backup Manager  ║
echo  ╚═══════════════════════════════════════════╝
echo.

echo  Checking Node.js...
node --version >nul 2>&1
if %errorlevel% neq 0 (
    color 0C
    echo  ERROR: Node.js is not installed.
    echo  Download from: https://nodejs.org/en/download
    pause
    exit /b 1
)
for /f "tokens=*" %%v in ('node --version') do echo  Found %%v

echo  Checking dependencies...
if not exist "node_modules\" (
    echo  Installing packages...
    call npm install
    if %errorlevel% neq 0 (
        echo  ERROR: npm install failed.
        pause
        exit /b 1
    )
)

echo.
echo  ─────────────────────────────────────────────
echo  Server starting on http://localhost:3420
echo  Browser opens automatically in 2 seconds.
echo  Press Ctrl+C to stop.
echo  ─────────────────────────────────────────────
echo.

if not exist "logs\" mkdir logs

start "" cmd /c "timeout /t 2 >nul && start http://localhost:3420"

node server.js

echo.
color 0C
echo  Server stopped. Check logs above for errors.
pause
