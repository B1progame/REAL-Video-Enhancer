from abc import abstractmethod
import os
import subprocess
import sys
import shutil
import argparse
import platform
import urllib.request
from importlib import metadata

PROJECT_ROOT = os.path.dirname(os.path.abspath(__file__))
os.chdir(PROJECT_ROOT)

PLATFORM = sys.platform
CPU_ARCH = "x86_64" if platform.machine() == "AMD64" else platform.machine()
OUTPUT_FOLDER = os.environ.get("RVE_OUTPUT_FOLDER", "release-stage").strip() or "release-stage"
BUILD_ENV = os.environ.get("RVE_BUILD_VENV") or (
    os.path.join(PROJECT_ROOT, ".venv")
    if os.path.isdir(os.path.join(PROJECT_ROOT, ".venv"))
    else os.path.join(PROJECT_ROOT, "venv")
)
PYTHON_IN_BUILD_ENV = os.path.join(BUILD_ENV, "Scripts", "python.exe") if PLATFORM == "win32" else os.path.join(BUILD_ENV, "bin", "python")
print(f"Platform: {PLATFORM}")
print(f"CPU Arch: {CPU_ARCH}")
print(f"OUTPUT_FOLDER: {OUTPUT_FOLDER}")


def set_mainwindow_size():
    import xml.etree.ElementTree as ET

    def set_mainwindow_size_zero(path="testRVEInterface.ui"):
        tree = ET.parse(path)
        root = tree.getroot()

        geometry = root.find('.//property[@name="geometry"]/rect')
        if geometry is not None:
            width = geometry.find("width")
            height = geometry.find("height")
            if width is not None:
                width.text = "1000"
            if height is not None:
                height.text = "700"
            tree.write(path)

    set_mainwindow_size_zero()


def download_file(url, destination):
        print(f"Downloading file from {url}")
        urllib.request.urlretrieve(url, destination)
        print("File downloaded successfully")

def get_libxcb_cursor_binary():
    try:
        if CPU_ARCH == "x86_64":
            input_file = '/usr/lib/x86_64-linux-gnu/libxcb-cursor.so.0'
        else:
            input_file = '/usr/lib/aarch64-linux-gnu/libxcb-cursor.so.0'
        if not os.path.isfile(input_file):
            raise FileNotFoundError("Unable to build as libxcbcursor is not installed!")
        
    except FileNotFoundError:
        try:
            print("libxcbcursor not found, downloading...")
            if CPU_ARCH == "x86_64":
                download_file("https://github.com/TNTwise/real-video-enhancer-models/releases/download/models/libxcb-cursor.so.0","libxcb-cursor.so.0")
            else:
                download_file("https://github.com/TNTwise/real-video-enhancer-models/releases/download/models/libxcb-cursor.so.0_arm64","libxcb-cursor.so.0")
            input_file = "libxcb-cursor.so.0"
        except Exception as e:
            print(e)
            raise FileNotFoundError("libxcbcursor not installed, and no network available to download it!")
    return input_file
    
class PythonManager:

    PYTHON_VENV_PATH = PYTHON_IN_BUILD_ENV
    PYTHON_SYSTEM_EXECUTABLE = sys.executable

    def __init__(self):
        pass
    
    @classmethod
    def run_venv_python(cls, command):
        args = [cls.PYTHON_VENV_PATH] + (command.split() if isinstance(command, str) else list(command))
        return subprocess.run(args, check=True)
    
    @classmethod
    def pip_install_package_in_venv(cls, package: str):
        command = [
            cls.PYTHON_VENV_PATH,
            "-m",
            "pip",
            "install",
            package,
        ]
        subprocess.run(command, check=True)

    def setup_python(self):
        self.__create_venv()
        self.__install_pip_in_venv()
        self.__install_requirements_in_venv()

    def __create_venv(self):
        print("Creating virtual environment")
        command = [self.PYTHON_SYSTEM_EXECUTABLE, "-m", "venv", BUILD_ENV]
        subprocess.run(command, check=True)


    def __install_pip_in_venv(self):
        command = [
            self.PYTHON_VENV_PATH,
            "-m",
            "ensurepip",
        ]
        subprocess.run(command, check=True)

    def __install_requirements_in_venv(self):
        print("Installing requirements in virtual environment")
        if not os.path.isfile("requirements.txt"):
            raise FileNotFoundError("No requirements.txt in current directory!")
        command = [
            self.PYTHON_VENV_PATH,
            "-m",
            "pip",
            "install",
            "-r",
            "requirements.txt",
        ]

        subprocess.run(command, check=True)
        

    def get_venv_site_packages(self):
        command = [
            self.PYTHON_VENV_PATH,
            "-c",
            'import site; print("\\n".join(site.getsitepackages()))',
        ]
        result = subprocess.run(command, stdout=subprocess.PIPE, text=True)
        site_packages = result.stdout.strip().split('\n')[0]
        if os.path.exists(site_packages):
            return site_packages
        site_packages = site_packages.replace('dist','site')
        if os.path.exists(site_packages):
            return site_packages
        print(site_packages)
        raise FileNotFoundError("Unable to locate site packages for python venv!")
    


class BuildManager:
    def __init__(self):
        self.python_manager = PythonManager()

    def build(self):
        ...
    
    def build_gui(self):
        print("Building GUI")
        #set_mainwindow_size()
        if PLATFORM == "darwin" or PLATFORM == "linux":
            os.system(
                f"{self.python_manager.get_venv_site_packages()}/PySide6/Qt/libexec/uic -g python testRVEInterface.ui > mainwindow.py"
            )
        if PLATFORM == "win32":
            os.system(
                r".\venv\Lib\site-packages\PySide6\uic.exe -g python testRVEInterface.ui > mainwindow.py"
            )
    
    def build_resources(self):
        print("Building resources.rc")
        if PLATFORM == "darwin" or PLATFORM == "linux":
            os.system(
                f"{self.python_manager.get_venv_site_packages()}/PySide6/Qt/libexec/rcc -g python resources.qrc > resources_rc.py"
            )
        if PLATFORM == "win32":
            os.system(
                r".\venv\Lib\site-packages\PySide6\rcc.exe -g python resources.qrc > resources_rc.py"
            )

    
    def copy_backend(self):
        print("Copying backend")
        if "pyinstaller" in self.__str__().lower():
            backend_dir = os.path.join(f"{OUTPUT_FOLDER}/REAL-Video-Enhancer/backend")
            try:
                shutil.copytree("backend",backend_dir)
            except Exception:
                raise FileNotFoundError("Backend failed to copy!")
            if not os.path.exists(backend_dir):
                raise FileNotFoundError("Backend failed to copy!")
        else:
            shutil.copytree("backend", f"{OUTPUT_FOLDER}/backend")

    @abstractmethod
    def patch_for_xcbcursor(self):
        ...

class PyInstaller(BuildManager):
    pyinstaller_version = "pyinstaller==6.22.3"

    @staticmethod
    def clean_previous_build_outputs():
        """Remove only this project's generated build directories before packaging."""
        generated_paths = (
            os.path.join(PROJECT_ROOT, "build"),
            os.path.join(PROJECT_ROOT, "dist"),
            os.path.join(PROJECT_ROOT, OUTPUT_FOLDER, "REAL-Video-Enhancer"),
        )
        for path in generated_paths:
            resolved = os.path.abspath(path)
            if os.path.commonpath((PROJECT_ROOT, resolved)) != PROJECT_ROOT:
                raise ValueError(f"Refusing to clean a path outside the project: {resolved}")
            if os.path.isdir(resolved):
                print(f"Removing previous generated build output: {resolved}")
                shutil.rmtree(resolved)

    def build(self):
        print("Building headless Python render service for the native Rust UI")
        if not os.path.isfile(PYTHON_IN_BUILD_ENV):
            self.python_manager.setup_python()
        self.clean_previous_build_outputs()
        try:
            installed_version = metadata.version("pyinstaller")
        except metadata.PackageNotFoundError:
            installed_version = None
        if installed_version != self.pyinstaller_version.split("==", 1)[1]:
            try:
                PythonManager.pip_install_package_in_venv(self.pyinstaller_version)
            except subprocess.CalledProcessError:
                # Keep a working offline build path when an older PyInstaller
                # is already present; surface the version in the build log.
                if not installed_version:
                    raise
                print(f"Could not update PyInstaller; using installed {installed_version}.")
        work_path = os.path.join("build", "pyinstaller-work")
        spec_path = os.path.join("build", "pyinstaller-spec")
        os.makedirs(work_path, exist_ok=True)
        os.makedirs(spec_path, exist_ok=True)
        separator = ";" if PLATFORM == "win32" else ":"
        # Ship only what the engine reads at run time: no tests, no bytecode.
        backend_stage = os.path.abspath(os.path.join("build", "backend-stage"))
        shutil.rmtree(backend_stage, ignore_errors=True)
        shutil.copytree(os.path.join(PROJECT_ROOT, "backend"), backend_stage,
                        ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "tests", ".pytest_cache"))
        command = [
            "-m", "PyInstaller", os.path.join(PROJECT_ROOT, "backend", "rve-service.py"), "--noconfirm", "--clean",
            "--onedir", "--windowed", "--name", "RVE-Engine",
            "--icon", os.path.join(PROJECT_ROOT, "icons", "logo-v3.ico"),
            "--distpath", os.path.join(OUTPUT_FOLDER, "REAL-Video-Enhancer"),
            "--workpath", work_path,
            "--specpath", spec_path,
            "--add-data", f"{backend_stage}{separator}backend",
            "--add-data", f"{os.path.join(PROJECT_ROOT, 'install-all-models-wsl.sh')}{separator}.",
        ]
        if PLATFORM == "win32":
            # Windows uses its native file picker; Tcl/Tk would add about 9 MB.
            command += ["--exclude-module", "tkinter", "--exclude-module", "_tkinter"]
        PythonManager.run_venv_python(command)

    def patch_for_xcbcursor(self):
        if PLATFORM == "linux":
            input_file = get_libxcb_cursor_binary()
            print("Copying libcursor to qt lib directory")
            shutil.copy(input_file, f"{OUTPUT_FOLDER}/REAL-Video-Enhancer/_internal/PySide6/Qt/lib/")
            
class CxFreeze(BuildManager):

    cx_freeze_version = "cx_freeze==7.2.10"

    def build(self):
        print("Building executable")

        PythonManager.pip_install_package_in_venv(self.cx_freeze_version)
        PythonManager.run_venv_python(
            (
              " -m"
            + " cx_Freeze"
            + " REAL-Video-Enhancer.py"
            + " --target-dir"
            + f" {OUTPUT_FOLDER}"
            )
        )

    def patch_for_xcbcursor(self):
        if PLATFORM == "linux":
            input_file = get_libxcb_cursor_binary()
            print("Copying libcursor to qt lib directory")
            shutil.copy(input_file, f"{OUTPUT_FOLDER}/lib/PySide6/Qt/lib")
            

class Nuitka(BuildManager):

    nuitka_version = "nuitka==2.6.7"

    def build(self):
        print("Building executable")

        PythonManager.pip_install_package_in_venv(self.nuitka_version)
        PythonManager.run_venv_python(
            (
              " -m nuitka" 
            + " --standalone" 
            + " --low-memory"
            + " --include-package-data=PySide6"
            + " --include-package-data=cpuinfo"
            + " --enable-plugin=pyside6"
            + " --include-qt-plugins=qml"
            + " --show-progress" 
            + " --show-scons" 
            + f" --output-dir={OUTPUT_FOLDER}"
            + " REAL-Video-Enhancer.py"
            )
        )

    def patch_for_xcbcursor(self):
        if PLATFORM == "linux":
            raise NotImplementedError("Nuitka is not working on linux.")
            input_file = get_libxcb_cursor_binary()
            print("Copying libcursor to qt lib directory")
            shutil.copy(input_file, f"{OUTPUT_FOLDER}/lib/PySide6/Qt/lib")


if __name__ == "__main__":
    args = argparse.ArgumentParser()
    args.add_argument("--run", help="Run the application", action="store_true")
    args.add_argument("--run_backend", help="Run the backend", action="store_true")
    args.add_argument("--build", help="Build the application with a specific builder.", default="gui", choices=["pyinstaller", "cx_freeze", "nuitka", "gui"])
    args.add_argument("--copy_backend", help="Copy the backend to the build directory", action="store_true")    
    args = args.parse_args()
    manager = BuildManager()
    if args.build == "pyinstaller":
        builder = PyInstaller()
        if args.run:
            raise SystemExit("--run is only valid with --build gui")
        builder.build()
        if args.copy_backend:
            builder.copy_backend()
        print("Headless Python render engine build complete")
    elif args.build == "gui":
        if not os.path.isfile(PYTHON_IN_BUILD_ENV):
            manager.python_manager.setup_python()
        manager.build_gui()
        manager.build_resources()
        if args.run:
            PythonManager.run_venv_python("REAL-Video-Enhancer.py")
        elif args.run_backend:
            PythonManager.run_venv_python("backend/rve-backend.py")
    else:
        raise SystemExit("The Windows release installer packages the headless Python render engine and native Rust UI.")
    
    
