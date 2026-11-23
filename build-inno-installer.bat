@echo off
setlocal

set "APP_DIR=%~dp0"
set "SCRIPT=%APP_DIR%REAL-Video-Enhancer.iss"
set "ISCC=%ProgramFiles(x86)%\Inno Setup 7\ISCC.exe"
set "PYTHON_EXE=%APP_DIR%.venv\Scripts\python.exe"

if not exist "%ISCC%" set "ISCC=%ProgramFiles%\Inno Setup 7\ISCC.exe"
if not exist "%ISCC%" set "ISCC=%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
if not exist "%ISCC%" set "ISCC=%ProgramFiles%\Inno Setup 6\ISCC.exe"
if not exist "%PYTHON_EXE%" set "PYTHON_EXE=%APP_DIR%venv\Scripts\python.exe"
if not exist "%PYTHON_EXE%" set "PYTHON_EXE=python"

if not exist "%ISCC%" (
    echo Inno Setup Compiler not found. Install Inno Setup 6 or 7 and rerun this script.
    exit /b 1
)
if not exist "%SCRIPT%" (
    echo Missing installer script: "%SCRIPT%"
    exit /b 1
)
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist "%VSWHERE%" (
    for /f "usebackq tokens=*" %%I in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSINSTALL=%%I"
)
if defined VSINSTALL if exist "%VSINSTALL%\Common7\Tools\VsDevCmd.bat" call "%VSINSTALL%\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul
where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust and Cargo are required to build the native interface. Install Rust from https://rustup.rs.
    exit /b 1
)
where link.exe >nul 2>nul
if errorlevel 1 (
    echo The Visual C++ linker is missing. Install the Visual Studio Desktop development with C++ workload, then rerun this script.
    exit /b 1
)

pushd "%APP_DIR%"
set "RVE_BUILD_VENV="
if /I "%PYTHON_EXE%"=="%APP_DIR%.venv\Scripts\python.exe" set "RVE_BUILD_VENV=%APP_DIR%.venv"
if /I "%PYTHON_EXE%"=="%APP_DIR%venv\Scripts\python.exe" set "RVE_BUILD_VENV=%APP_DIR%venv"

echo [1/3] Building the headless video engine...
"%PYTHON_EXE%" build.py --build pyinstaller
if errorlevel 1 goto :build_failed

set "RVE_VERSION_FILE=%TEMP%\rve-version-%RANDOM%-%RANDOM%.txt"
"%PYTHON_EXE%" -c "from src.version import version; print(version)" > "%RVE_VERSION_FILE%"
if errorlevel 1 goto :build_failed
set /p "RVE_VERSION="<"%RVE_VERSION_FILE%"
del "%RVE_VERSION_FILE%" >nul 2>&1
if not defined RVE_VERSION set "RVE_VERSION=2.4.37"
set "RVE_APP_VERSION=%RVE_VERSION%"

echo [2/3] Compiling the Rust desktop interface...
cargo build --release --manifest-path "native-ui\Cargo.toml"
if errorlevel 1 goto :build_failed

if not exist "release-stage\REAL-Video-Enhancer\RVE-Engine\RVE-Engine.exe" (
    echo The headless Python engine was not created.
    goto :build_failed
)
copy /Y "native-ui\target\release\rve-native.exe" "release-stage\REAL-Video-Enhancer\REAL-Video-Enhancer.exe" >nul
if errorlevel 1 goto :build_failed

echo [3/3] Compiling the Windows installer...
"%ISCC%" /DMyAppVersion=%RVE_VERSION% "REAL-Video-Enhancer.iss"
if errorlevel 1 goto :build_failed

popd
echo Installer created in release-stage\installer.
endlocal
exit /b 0

:build_failed
popd
echo Build failed. See the output above for details.
endlocal
exit /b 1
