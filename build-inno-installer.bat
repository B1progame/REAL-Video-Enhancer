@echo off
setlocal

set "APP_DIR=%~dp0"
set "BUILD_SCRIPT=%APP_DIR%build.py"
set "SCRIPT=%APP_DIR%REAL-Video-Enhancer.iss"
set "ISCC=%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
set "DIST_APP_DIR=%APP_DIR%dist\REAL-Video-Enhancer"
set "DIST_EXE=%DIST_APP_DIR%\REAL-Video-Enhancer.exe"
set "PYTHON_EXE=%APP_DIR%.venv\Scripts\python.exe"

if not exist "%ISCC%" set "ISCC=%ProgramFiles%\Inno Setup 6\ISCC.exe"
if not exist "%PYTHON_EXE%" set "PYTHON_EXE=%APP_DIR%venv\Scripts\python.exe"
if not exist "%PYTHON_EXE%" set "PYTHON_EXE=python"

if not exist "%ISCC%" (
    echo Inno Setup Compiler not found.
    echo Install Inno Setup 6 and rerun this script.
    exit /b 1
)

if not exist "%SCRIPT%" (
    echo Missing installer script: "%SCRIPT%"
    exit /b 1
)

if not exist "%BUILD_SCRIPT%" (
    echo Missing build script: "%BUILD_SCRIPT%"
    exit /b 1
)

echo [1/2] Building app with PyInstaller...
pushd "%APP_DIR%"
"%PYTHON_EXE%" "build.py" --build pyinstaller --copy_backend
if errorlevel 1 (
    popd
    echo Build failed.
    exit /b 1
)
popd

if not exist "%DIST_EXE%" (
    echo Build finished but executable is missing:
    echo   "%DIST_EXE%"
    exit /b 1
)

echo [2/2] Building Inno installer...
"%ISCC%" "%SCRIPT%"
if errorlevel 1 (
    echo Inno installer compilation failed.
    exit /b 1
)

echo Done.
