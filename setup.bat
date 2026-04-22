@echo off
setlocal

echo ==========================================
echo    Vanguard Engine v3 Setup Utility
echo ==========================================

:: Check for Cargo
cargo --version >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] Rust/Cargo not found. Please install from https://rustup.rs/
    pause
    exit /b 1
)

echo [1/2] Building Vanguard Engine in Release mode...
echo This may take a few minutes for the first build.
cargo build --release

if %errorlevel% neq 0 (
    echo [ERROR] Build failed. Check the logs above.
    pause
    exit /b 1
)

echo [2/2] Creating standalone executable link...
copy target\release\vanguard_engine_v3.exe vanguard_engine_v3.exe /Y

echo.
echo ==========================================
echo    Setup Complete!
echo    Run 'vanguard_engine_v3.exe' to start.
echo ==========================================
pause
