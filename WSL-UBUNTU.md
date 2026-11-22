# Windows app with models in WSL Ubuntu

The native Rust desktop app and its Python video engine run on Windows. On first
launch, the app installs its managed Windows Python runtime and asks where to
store models. Settings lets you choose WSL or Windows storage. WSL is the
default; when selected, the app starts the distro if needed, installs and
verifies the complete model catalog, and reads weights through the WSL network
share. The default Windows path is
`\\wsl.localhost\Ubuntai\home\Benjai\.local\share\REAL-Video-Enhancer\models`.
Set `RVE_WSL_DISTRO`, `RVE_WSL_USER`, or `RVE_MODEL_DIR` to change that location.

From the repository folder inside Ubuntu, install system packages once:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config python3 python3-venv python3-pip \
  curl ffmpeg libgl1 libx11-dev libxkbcommon-dev libxkbcommon-x11-0 \
  libwayland-dev libvulkan1
```

Install the full model catalog into the WSL home directory:

```bash
bash install-all-models-wsl.sh
```

Run the Windows installer and launch the app. To manually verify or reinstall
the WSL model catalog, run:

```bash
bash install-all-models-wsl.sh
```

The default WSL weights directory is `~/.local/share/REAL-Video-Enhancer/models`.
Set `RVE_MODEL_DIR` to choose another model location or
`RVE_MODEL_WORKERS=2` to reduce simultaneous downloads. Choosing Windows storage
puts all available weights in `%LOCALAPPDATA%\REAL-Video-Enhancer\models`.
