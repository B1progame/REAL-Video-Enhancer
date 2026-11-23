"""Loopback API bridge for the native Rust desktop UI.

The bridge uses only Python's standard library. It deliberately reports optional
runtime/model operations as unavailable when this checkout cannot perform them.
"""
from __future__ import annotations

import json
import multiprocessing.shared_memory as shared_memory
import os
import platform
import shutil
import shlex
import struct
import subprocess
import sys
import tarfile
import zipfile
import tempfile
import threading
import time
import uuid
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

FROZEN = bool(getattr(sys, "frozen", False))
ROOT = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parents[1])).resolve()
BACKEND = ROOT / "backend" / "rve-backend.py"
LOCAL_DATA = Path(os.environ.get("LOCALAPPDATA", Path.home())) / "REAL-Video-Enhancer"
if os.environ.get("RVE_APP_DATA"):
    APP_DATA = Path(os.environ["RVE_APP_DATA"]).expanduser()
elif FROZEN:
    APP_DATA = LOCAL_DATA
elif sys.platform.startswith("linux"):
    APP_DATA = Path.home() / ".local" / "share" / "REAL-Video-Enhancer"
else:
    APP_DATA = ROOT
SETTINGS_FILE = APP_DATA / "web-settings.json" if FROZEN else ROOT / "web-ui" / ".web-settings.json"
PORT = int(os.environ.get("RVE_API_PORT", "0" if FROZEN else "51837"))
LEGACY_PROGRAMS = Path(os.environ.get("LOCALAPPDATA", Path.home())) / "Programs"
managed_python_name = "python.exe" if os.name == "nt" else "bin/python3"
runtime_candidates = [
    LOCAL_DATA / "python" / "python" / managed_python_name,
    LOCAL_DATA / "python" / "python" / ("Scripts/python.exe" if os.name == "nt" else "bin/python"),
    LEGACY_PROGRAMS / "REAL Video Enhancer" / "python" / "python" / managed_python_name,
    LEGACY_PROGRAMS / "REAL Video Enhancer-old" / "python" / "python" / managed_python_name,
    ROOT / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python"),
]
default_python = next((str(item) for item in runtime_candidates if item.is_file()), None)
default_python = default_python or shutil.which("python") or shutil.which("python3")
# In a frozen engine, sys.executable is the engine, not Python. Falling back to
# it would recursively launch the service as the renderer.
if not default_python and not FROZEN:
    default_python = sys.executable
PYTHON = os.environ.get("RVE_PYTHON", default_python or "")
managed_python_paths = {path.resolve() for path in runtime_candidates if path.is_absolute()}
if PYTHON and Path(PYTHON).resolve() in managed_python_paths:
    os.environ["PYTHONNOUSERSITE"] = "1"
state_lock = threading.RLock()
capability_cache_lock = threading.RLock()
capability_cache = {"checked_at": 0.0, "value": None}
render = {"status": "idle", "paused": False, "process": None, "pause_memory": None, "preview_memory": None, "preview_jpeg": None, "preview_size": 0, "preview_enabled": False, "engine": "rve", "log_lines": [], "started": 0.0, "output_file": None, "current_frame": 0, "gen_fps": None, "eta": "--"}
render_cancel_event = threading.Event()
install_state = {"status": "not_started", "progress": 0, "stage": "Runtime check", "detail": "Runtime setup has not been started."}
install_lock = threading.RLock()
PYTHON_VERSION = "3.12.9"
UPDATE_REPOSITORY = "B1progame/REAL-Video-Enhancer"
UPDATE_API = f"https://api.github.com/repos/{UPDATE_REPOSITORY}/releases/latest"
UPDATE_DOWNLOAD_PREFIX = f"https://github.com/{UPDATE_REPOSITORY}/releases/download/"
APP_VERSION = os.environ.get("RVE_APP_VERSION", "2.4.46")
update_lock = threading.RLock()
update_state = {"status": "idle", "progress": 0,
                "detail": "Check B1progame on GitHub for the latest Windows release.",
                "current_version": APP_VERSION}
def default_model_dir(storage=None):
    configured = os.environ.get("RVE_MODEL_DIR")
    if configured:
        return Path(configured).expanduser()
    storage = (storage or os.environ.get("RVE_MODEL_STORAGE") or "wsl").lower()
    if storage == "windows":
        return APP_DATA / "models"
    # WSL exposes the Linux model directory as a normal UNC path to Windows.
    if os.name == "nt":
        distro = os.environ.get("RVE_WSL_DISTRO", "Ubuntai")
        linux_user = os.environ.get("RVE_WSL_USER", "Benjai")
        return Path(rf"\\wsl.localhost\{distro}\home\{linux_user}\.local\share\REAL-Video-Enhancer\models")
    return APP_DATA / "models"


MODEL_DIR = default_model_dir()
storage_lock = threading.RLock()
storage_job = {"status": "idle", "mode": os.environ.get("RVE_MODEL_STORAGE", "wsl" if os.name == "nt" else "windows"),
               "progress": 0, "stage": "Model storage", "detail": "Model storage has not been prepared.", "error": ""}


def custom_models_dir():
    if os.name == "nt" and os.environ.get("RVE_MODEL_STORAGE", "wsl").lower() != "windows":
        return MODEL_DIR / "custom_models"
    return APP_DATA / "custom_models"
MODEL_DOWNLOAD_BASE = "https://github.com/TNTwise/real-video-enhancer-models/releases/download/models/"
download_lock = threading.RLock()
download_job = {"id": None, "status": "idle", "active": False, "progress": 0, "label": "", "detail": "", "error": "", "results": [], "bytes_received": 0, "bytes_total": 0, "stage": ""}
cancel_event = threading.Event()
runtime_process_lock = threading.RLock()
runtime_process = None
storage_process_lock = threading.RLock()
storage_process = None
UV_VERSION = "0.13.7"
UV_SHA256_X64 = "0cdc5b38fe5a8168ed6a25814f5ba43c21d313dd3187314fccb37b7cec46e021"
update_cancel_event = threading.Event()
update_process_lock = threading.RLock()
update_process = None


def hidden_subprocess_kwargs():
    """Keep CLI probes and render children from opening Windows consoles."""
    if os.name != "nt":
        return {}
    startupinfo = subprocess.STARTUPINFO()
    startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startupinfo.wShowWindow = 0
    return {"creationflags": getattr(subprocess, "CREATE_NO_WINDOW", 0), "startupinfo": startupinfo}


def version_tuple(value):
    import re
    match = re.search(r"\d+(?:\.\d+)+", str(value))
    return tuple(int(part) for part in match.group(0).split(".")) if match else ()


def github_cli_path():
    return shutil.which("gh.exe") or shutil.which("gh")


def fetch_latest_release():
    request = urllib.request.Request(UPDATE_API, headers={"Accept": "application/vnd.github+json", "User-Agent": "REAL-Video-Enhancer-Native"})
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            return json.loads(response.read(2 * 1024 * 1024)), None
    except urllib.error.HTTPError as exc:
        if exc.code not in (401, 403, 404):
            raise
        gh = github_cli_path()
        if not gh:
            raise RuntimeError(
                "This GitHub repository is private. Install GitHub CLI and sign in with an account "
                "that can read B1progame/REAL-Video-Enhancer, then try again."
            ) from exc
        result = subprocess.run(
            [gh, "api", f"repos/{UPDATE_REPOSITORY}/releases/latest"],
            capture_output=True, text=True, timeout=20, **hidden_subprocess_kwargs(),
        )
        if result.returncode:
            detail = result.stderr.strip().splitlines()[-1] if result.stderr.strip() else "GitHub CLI could not read the release."
            raise RuntimeError(
                "The private GitHub release could not be read. Sign in with `gh auth login -h github.com` "
                f"using an account with repository access. {detail}"
            ) from exc
        try:
            return json.loads(result.stdout), "gh"
        except json.JSONDecodeError as error:
            raise RuntimeError("GitHub CLI returned an invalid release response.") from error


def check_github_update():
    release, access_method = fetch_latest_release()
    release_tag = str(release.get("tag_name") or "")
    tag = release_tag.lstrip("v")
    if not tag:
        raise RuntimeError("GitHub did not return a release version.")
    installer = next((asset for asset in release.get("assets", [])
                      if str(asset.get("name", "")).lower().endswith(".exe")
                      and "windows" in str(asset.get("name", "")).lower()
                      and ("setup" in str(asset.get("name", "")).lower() or "installer" in str(asset.get("name", "")).lower())), None)
    newer = bool(version_tuple(tag) and version_tuple(APP_VERSION) and version_tuple(tag) > version_tuple(APP_VERSION))
    digest = str(installer.get("digest") or "") if installer else ""
    installer_sha256 = digest.removeprefix("sha256:").lower() if digest.startswith("sha256:") else ""
    has_valid_digest = len(installer_sha256) == 64 and all(char in "0123456789abcdef" for char in installer_sha256)
    can_download = bool(newer and installer and has_valid_digest)
    return {"status": "ready", "current_version": APP_VERSION, "latest_version": tag, "release_tag": release_tag,
            "update_available": can_download, "release_url": release.get("html_url"),
            "installer_name": installer.get("name") if installer else None,
            "installer_url": installer.get("browser_download_url") if installer else None,
            "installer_size": installer.get("size") if installer else None,
            "installer_sha256": installer_sha256 if has_valid_digest else None,
            "access_method": access_method,
            "detail": (f"Version {tag} is ready to install." if can_download else
                       f"Version {tag} is newer, but GitHub did not provide a valid installer checksum." if newer and installer and not has_valid_digest else
                       f"Version {tag} is newer, but GitHub has no Windows installer asset." if newer else
                       f"You have {APP_VERSION}; the latest stable B1progame release is {tag}." if version_tuple(tag) else
                       "Could not compare the release versions.")}


def download_update_installer(release):
    try:
        url = str(release.get("installer_url") or "")
        if not url.startswith(UPDATE_DOWNLOAD_PREFIX):
            raise ValueError("GitHub did not provide a trusted Windows installer download.")
        name = Path(str(release.get("installer_name") or "")).name
        if not name.lower().endswith(".exe"):
            raise ValueError("The Windows release installer was not found.")
        expected_size = int(release.get("installer_size") or 0)
        expected_sha256 = str(release.get("installer_sha256") or "").lower()
        if expected_size <= 0 or len(expected_sha256) != 64:
            raise ValueError("GitHub did not provide a valid installer size and SHA-256 checksum.")
        folder = APP_DATA / "updates"
        folder.mkdir(parents=True, exist_ok=True)
        target = folder / name
        partial = target.with_suffix(target.suffix + ".part")
        import hashlib
        checksum = hashlib.sha256()
        received = 0
        if release.get("access_method") == "gh":
            gh = github_cli_path()
            if not gh:
                raise RuntimeError("GitHub CLI is required to download from this private repository.")
            tag = str(release.get("release_tag") or "")
            if not tag:
                raise ValueError("GitHub release metadata did not include the original release tag.")
            temporary_folder = folder / f".download-{uuid.uuid4().hex}"
            temporary_folder.mkdir(parents=True, exist_ok=True)
            try:
                global update_process
                log_path = temporary_folder / "gh-download.log"
                with log_path.open("w", encoding="utf-8") as log_stream:
                    proc = subprocess.Popen(
                        [gh, "release", "download", tag, "--repo", UPDATE_REPOSITORY,
                         "--pattern", name, "--dir", str(temporary_folder), "--clobber"],
                        stdout=log_stream, stderr=subprocess.STDOUT, **hidden_subprocess_kwargs(),
                    )
                    with update_process_lock:
                        update_process = proc
                    started = time.monotonic()
                    timed_out = False
                    while proc.poll() is None:
                        if update_cancel_event.is_set():
                            proc.terminate()
                        if time.monotonic() - started > 600:
                            proc.terminate()
                            timed_out = True
                            break
                        time.sleep(0.1)
                    code = proc.wait()
                    with update_process_lock:
                        update_process = None
                if timed_out:
                    raise TimeoutError("GitHub CLI update download exceeded ten minutes.")
                if update_cancel_event.is_set():
                    raise InterruptedError("Update download cancelled.")
                if code:
                    log = log_path.read_text(encoding="utf-8", errors="replace").strip().splitlines()
                    detail = log[-1] if log else "GitHub CLI download failed."
                    raise RuntimeError(f"Could not download the private release with GitHub CLI: {detail}")
                downloaded = temporary_folder / name
                if not downloaded.is_file():
                    raise FileNotFoundError("GitHub CLI did not download the expected Windows installer.")
                downloaded.replace(partial)
            finally:
                shutil.rmtree(temporary_folder, ignore_errors=True)
            with partial.open("rb") as source:
                while True:
                    if update_cancel_event.is_set():
                        raise InterruptedError("Update download cancelled.")
                    block = source.read(1024 * 1024)
                    if not block:
                        break
                    checksum.update(block)
                    received += len(block)
                    if received > expected_size:
                        raise IOError("The installer download exceeded the published release size.")
                    with update_lock:
                        update_state["progress"] = min(100, round(received * 100 / expected_size))
        else:
            request = urllib.request.Request(url, headers={"User-Agent": "REAL-Video-Enhancer-Native"})
            with urllib.request.urlopen(request, timeout=45) as response, partial.open("wb") as output:
                total = int(response.headers.get("Content-Length", "0") or 0)
                if total and total != expected_size:
                    raise IOError(f"GitHub reported {total} bytes, but the release metadata says {expected_size}.")
                while True:
                    if update_cancel_event.is_set():
                        raise InterruptedError("Update download cancelled.")
                    block = response.read(1024 * 1024)
                    if not block:
                        break
                    output.write(block)
                    checksum.update(block)
                    received += len(block)
                    if received > expected_size:
                        raise IOError("The installer download exceeded the published release size.")
                    with update_lock:
                        update_state["progress"] = min(100, round(received * 100 / expected_size))
        if received != expected_size:
            raise IOError(f"The installer download was incomplete ({received} of {expected_size} bytes).")
        if checksum.hexdigest() != expected_sha256:
            raise IOError("The downloaded installer does not match GitHub's SHA-256 checksum.")
        if target.exists():
            target.unlink()
        partial.replace(target)
        with update_lock:
            update_state.update(status="downloaded", progress=100, installer_path=str(target),
                                detail=f"{name} is downloaded and ready to install.")
    except Exception as exc:
        try:
            partial.unlink(missing_ok=True)
        except (UnboundLocalError, OSError):
            pass
        with update_lock:
            update_state.update(status="cancelled" if isinstance(exc, InterruptedError) else "error",
                                detail="Update download cancelled." if isinstance(exc, InterruptedError) else f"Update download failed: {exc}")
        update_cancel_event.clear()


def begin_update_download():
    with update_lock:
        if update_state.get("status") == "downloading":
            return dict(update_state)
        release = update_state.get("release")
        if not release or not release.get("update_available") or not release.get("installer_url"):
            update_state.update(status="error", error="No newer Windows installer is available.",
                                detail="Check for a newer release first. There is no newer Windows installer to download.")
            return dict(update_state)
        update_state.update(status="downloading", progress=0, detail="Downloading the GitHub Windows installer…")
        update_cancel_event.clear()
        threading.Thread(target=download_update_installer, args=(release,), name="rve-update-download", daemon=True).start()
        return dict(update_state)


def find_ffmpeg_path():
    executable = "ffmpeg.exe" if os.name == "nt" else "ffmpeg"
    candidates = [APP_DATA / "bin" / executable, APP_DATA / executable]
    for install in (LEGACY_PROGRAMS / "REAL Video Enhancer", LEGACY_PROGRAMS / "REAL Video Enhancer-old"):
        candidates.extend((install / "bin" / executable, install / executable))
    bundled = next((str(path.resolve()) for path in candidates if path.is_file()), None)
    return bundled or shutil.which("ffmpeg")


def model_catalog(backend="pytorch"):
    backend = str(backend or "pytorch").lower().split()[0]
    if backend == "auto":
        backend = "pytorch"
    catalog_path = ROOT / "backend" / "model-catalog.json"
    try:
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        # This generated data file is shipped with the app. Keeping the bridge
        # independent of src.ModelHandler prevents PyInstaller from bundling
        # the old Qt desktop interface into the native Rust app.
        catalog = {}
    selected = catalog.get(backend) or catalog.get("pytorch") or {}
    selected = {key: dict(value) for key, value in selected.items()}
    # Preserve user-installed custom weights in the native model pickers.
    for custom in custom_model_roots():
        if custom.is_dir() and backend in ("pytorch", "tensorrt"):
            for item in custom.iterdir():
                if item.is_file() and item.suffix.lower() in (".pth", ".pt", ".safetensors"):
                    selected.setdefault("upscale", {})[item.name] = [item.name, item.name, 4, "custom"]
        elif custom.is_dir() and backend == "ncnn":
            for item in custom.iterdir():
                if item.is_dir():
                    selected.setdefault("upscale", {})[item.name] = [item.name, item.name, 4, "custom"]
    return backend, selected


def custom_model_roots():
    local = Path(os.environ.get("LOCALAPPDATA", str(Path.home())))
    roots = [custom_models_dir(), local / "Programs" / "REAL Video Enhancer-old" / "custom_models",
             local / "Programs" / "REAL Video Enhancer" / "custom_models", local / "REAL Video Enhancer" / "custom_models"]
    return list(dict.fromkeys(path.resolve() for path in roots))


def model_roots():
    local = Path(os.environ.get("LOCALAPPDATA", str(Path.home())))
    roots = [MODEL_DIR, custom_models_dir(), local / "REAL-Video-Enhancer" / "models",
             local / "REAL-Video-Enhancer" / "custom_models", local / "REAL Video Enhancer" / "models",
             local / "Programs" / "REAL Video Enhancer" / "models",
             local / "Programs" / "REAL Video Enhancer" / "custom_models",
             local / "Programs" / "REAL Video Enhancer-old" / "models",
             local / "Programs" / "REAL Video Enhancer-old" / "custom_models"]
    return list(dict.fromkeys(path.resolve() for path in roots))


def find_model_file(filename):
    for root in model_roots():
        candidate = root / filename
        if candidate.exists():
            return candidate
    return None


def catalog_model_status(catalog):
    result = {}
    for category, entries in catalog.items():
        result[category] = {}
        for label, spec in entries.items():
            result[category][label] = bool(find_model_file(spec[0]))
    return result


def safe_extract_model_archive(archive_path, destination):
    destination_resolved = destination.resolve()
    with tarfile.open(archive_path, "r:gz") as archive:
        members = archive.getmembers()
        for member in members:
            target = (destination / member.name).resolve()
            if target != destination_resolved and destination_resolved not in target.parents:
                raise ValueError(f"Model archive contains an unsafe path: {member.name}")
            if member.issym() or member.islnk() or member.isdev():
                raise ValueError(f"Model archive contains an unsupported link/device: {member.name}")
        archive.extractall(destination, members=members)


def _download_progress(label, index, count, received=0, total_bytes=0, stage="Downloading"):
    file_fraction = min(1.0, received / total_bytes) if total_bytes else 0.0
    overall = ((index - 1) + file_fraction) * 100 / max(count, 1)
    if total_bytes:
        detail = f"Model {index} of {count} · {received / (1024 ** 2):.1f} MB of {total_bytes / (1024 ** 2):.1f} MB"
    else:
        detail = f"Model {index} of {count} · {received / (1024 ** 2):.1f} MB downloaded"
    with download_lock:
        download_job.update(active=True, status="downloading", label=label, progress=round(overall, 1),
                            detail=detail, bytes_received=received, bytes_total=total_bytes, stage=stage)


def download_one_model(label, spec, index=1, count=1):
    filename, download_name = str(spec[0]), str(spec[1])
    if filename in ("", ".", "..") or Path(filename).is_absolute() or ".." in Path(filename).parts:
        raise ValueError(f"Invalid model filename for {label}.")
    existing = find_model_file(filename)
    if existing:
        return {"label": label, "status": "already_installed", "path": str(existing)}
    MODEL_DIR.mkdir(parents=True, exist_ok=True)
    target = MODEL_DIR / download_name
    partial = target.with_name(target.name + ".part")
    url = MODEL_DOWNLOAD_BASE + urllib.parse.quote(download_name)
    _download_progress(label, index, count)
    try:
        with urllib.request.urlopen(url, timeout=30) as response, partial.open("wb") as output:
            total = int(response.headers.get("Content-Length", "0") or 0)
            received = 0
            while True:
                if cancel_event.is_set():
                    raise InterruptedError("Model download cancelled.")
                chunk = response.read(1024 * 1024)
                if not chunk:
                    break
                output.write(chunk)
                received += len(chunk)
                _download_progress(label, index, count, received, total)
    except Exception:
        partial.unlink(missing_ok=True)
        raise
    _download_progress(label, index, count, received, total, "Installing")
    if target.suffixes[-2:] == [".tar", ".gz"]:
        try:
            safe_extract_model_archive(partial, MODEL_DIR)
            packed_folder = MODEL_DIR / download_name.removesuffix(".tar.gz")
            if filename != download_name and not (MODEL_DIR / filename).exists() and packed_folder.is_dir():
                shutil.copytree(packed_folder, MODEL_DIR / filename)
        finally:
            partial.unlink(missing_ok=True)
    else:
        partial.replace(target)
    installed = find_model_file(filename)
    if not installed:
        raise RuntimeError(f"Download completed, but expected model file/folder was not found: {filename}")
    return {"label": label, "status": "downloaded", "path": str(installed)}


def run_model_download(job_id, backend, selections=None, categories=None):
    try:
        if backend == "all":
            catalogs = {name: model_catalog(name)[1] for name in ("pytorch", "ncnn", "directml", "tensorrt")}
        else:
            catalogs = {backend: model_catalog(backend)[1]}
        requested = []
        if selections:
            for category, label in selections.items():
                if not label:
                    continue
                spec = next((items.get(category, {}).get(label) for items in catalogs.values()
                             if items.get(category, {}).get(label) is not None), None)
                if spec is None:
                    # A local/custom model is already usable without a remote download.
                    custom_path = custom_models_dir() / str(label)
                    if custom_path.exists():
                        continue
                    raise ValueError(f"Unknown {category} model: {label}")
                requested.append((label, spec))
        else:
            for catalog in catalogs.values():
                selected_categories = categories or list(catalog)
                for category in selected_categories:
                    for label, spec in catalog.get(category, {}).items():
                        requested.append((label, spec))
        deduplicated = {}
        for label, spec in requested:
            deduplicated.setdefault((str(spec[0]), str(spec[1])), (label, spec))
        requested = list(deduplicated.values())
        total = len(requested)
        results, errors = [], []
        for index, (label, spec) in enumerate(requested, 1):
            with download_lock:
                download_job.update(active=True, status="downloading", progress=round((index - 1) * 100 / max(total, 1), 1), label=label,
                                    detail=f"Model {index} of {total} · preparing download", bytes_received=0, bytes_total=0, stage="Downloading")
            try:
                result = download_one_model(label, spec, index, total)
                results.append(result)
            except Exception as exc:
                if isinstance(exc, InterruptedError):
                    break
                errors.append(f"{label}: {exc}")
            with download_lock:
                download_job["results"] = results[:]
                download_job.update(progress=round(index * 100 / total) if total else 100,
                                    detail=f"Model {index} of {total} ready", stage="Installing")
        with download_lock:
            cancelled = cancel_event.is_set()
            download_job.update(active=False, status="cancelled" if cancelled else "error" if errors else "completed", error="; ".join(errors),
                                detail="Model download cancelled. Partial files were removed." if cancelled else f"{len(results)} model(s) ready" + (f"; {len(errors)} failed" if errors else ""), progress=100 if not cancelled else download_job.get("progress", 0),
                                results=results)
            cancel_event.clear()
    except Exception as exc:
        with download_lock:
            download_job.update(active=False, status="error", error=str(exc), detail=str(exc), progress=0)


def start_model_download(backend, selections=None, categories=None):
    with download_lock:
        if download_job.get("active"):
            raise ValueError("A model download is already in progress.")
        if install_state.get("status") == "running" or (storage_job.get("status") in ("starting", "installing") and threading.current_thread().name != "rve-model-storage"):
            raise ValueError("Wait for runtime or storage setup to finish before downloading models.")
        cancel_event.clear()
        job_id = uuid.uuid4().hex
        download_job.update(id=job_id, active=True, status="queued", progress=0, label="", detail="Preparing model download…", error="", results=[])
    threading.Thread(target=run_model_download, args=(job_id, backend, selections, categories), daemon=True).start()
    return {key: value for key, value in download_job.items() if key != "results"}


def ensure_render_models(payload):
    if str(payload.get("engine") or "rve").lower() == "animescripter":
        return
    toggles, chosen = payload.get("toggles") or {}, payload.get("models") or {}
    backend = str(payload.get("backend") or "pytorch").lower()
    if backend == "auto":
        backend = "pytorch"
    _, catalog = model_catalog(backend)
    requested = [(category, str(chosen.get(category) or "")) for category in ("interpolate", "upscale", "decompress", "denoise", "deblur") if toggles.get(category)]
    missing = [(category, label) for category, label in requested if label and not find_model_file(catalog.get(category, {}).get(label, [label])[0])]
    if not missing:
        return
    with download_lock:
        if download_job.get("active"):
            raise ValueError("Wait for the current model download to finish before starting a render.")
        download_job.update(id=uuid.uuid4().hex, active=True, status="downloading", progress=0, label="",
                            detail="Preparing models required by this render", error="", results=[], bytes_received=0, bytes_total=0, stage="Preparing")
    errors = []
    try:
        for index, (category, label) in enumerate(missing, 1):
            spec = catalog.get(category, {}).get(label)
            if not spec:
                errors.append(f"Unknown {category} model: {label}")
                continue
            with download_lock:
                download_job.update(label=label, progress=round((index - 1) * 100 / len(missing), 1),
                                    detail=f"Model {index} of {len(missing)} · preparing download", bytes_received=0, bytes_total=0, stage="Downloading")
            try:
                download_one_model(label, spec, index, len(missing))
            except Exception as exc:
                if isinstance(exc, InterruptedError):
                    break
                errors.append(f"{label}: {exc}")
            with download_lock:
                download_job.update(progress=round(index * 100 / len(missing), 1), detail=f"Model {index} of {len(missing)} ready", stage="Installing")
    finally:
        with download_lock:
            cancelled = cancel_event.is_set()
            download_job.update(active=False, status="cancelled" if cancelled else "error" if errors else "completed", error="; ".join(errors), progress=download_job.get("progress", 0) if cancelled else 100,
                                detail="Required model download cancelled." if cancelled else "; ".join(errors) if errors else f"{len(missing)} required model(s) ready", stage="Cancelled" if cancelled else "Failed" if errors else "Complete")
            if cancelled:
                cancel_event.clear()
    if errors:
        raise ValueError("Unable to prepare selected model(s): " + "; ".join(errors))


def respond_json(handler, value, status=200):
    body = json.dumps(value, ensure_ascii=False).encode("utf-8")
    handler.send_response(status)
    handler.send_header("Content-Type", "application/json; charset=utf-8")
    handler.send_header("Content-Length", str(len(body)))
    handler.send_header("Cache-Control", "no-store")
    handler.end_headers()
    handler.wfile.write(body)


def load_settings():
    try:
        return json.loads(SETTINGS_FILE.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def save_settings(settings):
    SETTINGS_FILE.parent.mkdir(parents=True, exist_ok=True)
    temp = SETTINGS_FILE.with_suffix(".tmp")
    temp.write_text(json.dumps(settings, indent=2, ensure_ascii=False), encoding="utf-8")
    temp.replace(SETTINGS_FILE)


def storage_snapshot():
    with storage_lock:
        return {**storage_job, "model_dir": str(MODEL_DIR)}


def _set_storage_status(**updates):
    with storage_lock:
        storage_job.update(updates)


def delete_managed_data(kind):
    global PYTHON
    if kind not in ("python", "models"):
        raise ValueError("Choose Python or models for removal.")
    if render.get("status") in ("preparing", "running", "paused", "stopping") or download_job.get("active") or install_state.get("status") == "running" or storage_job.get("status") in ("starting", "installing"):
        raise ValueError("Wait for active work to stop before removing managed files.")
    if kind == "python":
        root = (LOCAL_DATA / "python").resolve()
        expected = (LOCAL_DATA / "python").resolve()
        if root != expected or root.name.lower() != "python" or root.parent != LOCAL_DATA.resolve():
            raise ValueError("Managed Python path failed its safety check.")
        if root.exists():
            shutil.rmtree(root)
        uv_cache = LOCAL_DATA / "uv-cache"
        if uv_cache.exists():
            shutil.rmtree(uv_cache)
        PYTHON = ""
        capability_cache.update(checked_at=0.0, value=None)
        return {"ok": True, "detail": "Managed Python and its installed Python packages were removed. Use Runtime setup to install them again."}

    if os.name == "nt" and storage_job.get("mode") == "wsl":
        distro = os.environ.get("RVE_WSL_DISTRO", "Ubuntai")
        if not all(ch.isalnum() or ch in "._-" for ch in distro):
            raise ValueError("The configured WSL name is not safe to use for model cleanup.")
        wsl = shutil.which("wsl.exe") or "wsl.exe"
        home_result = subprocess.run([wsl, "--distribution", distro, "--exec", "bash", "-lc", "printf %s \"$HOME\""],
                                     capture_output=True, text=True, timeout=60, **hidden_subprocess_kwargs())
        linux_home = home_result.stdout.strip().replace("\r", "")
        if home_result.returncode or not linux_home.startswith("/home/") or ".." in Path(linux_home).parts:
            raise RuntimeError("Could not safely locate the WSL user's home directory for model cleanup.")
        linux_path = linux_home.rstrip("/") + "/.local/share/REAL-Video-Enhancer/models"
        result = subprocess.run([wsl, "--distribution", distro, "--exec", "rm", "-rf", "--", linux_path, linux_path + "/custom_models"],
                                capture_output=True, text=True, timeout=180, **hidden_subprocess_kwargs())
        if result.returncode:
            raise RuntimeError((result.stderr or "WSL could not remove the managed model folder.").strip())
    else:
        root = (APP_DATA / "models").resolve()
        if root.name.lower() != "models" or root.parent != APP_DATA.resolve():
            raise ValueError("Managed model path failed its safety check.")
        if root.exists():
            shutil.rmtree(root)
        custom = APP_DATA / "custom_models"
        if custom.exists():
            shutil.rmtree(custom)
    return {"ok": True, "detail": "The managed model library and custom model files were removed. You can download models again from Downloads."}


def prepare_model_storage(mode):
    global MODEL_DIR
    try:
        if mode == "windows":
            MODEL_DIR = APP_DATA / "models"
            MODEL_DIR.mkdir(parents=True, exist_ok=True)
            _set_storage_status(mode=mode, stage="Installing models on Windows", progress=0,
                                detail="Preparing the complete model library in Local AppData.", error="")
            job = start_model_download("all")
            while True:
                with download_lock:
                    current = dict(download_job)
                _set_storage_status(progress=current.get("progress", 0), detail=current.get("detail", "Installing models"))
                if not current.get("active"):
                    if current.get("status") == "cancelled":
                        _set_storage_status(status="cancelled", stage="Setup cancelled", detail="Model setup was cancelled.", error="")
                        cancel_event.clear()
                        return
                    if current.get("status") == "error":
                        raise RuntimeError(current.get("error") or "One or more models could not be installed.")
                    break
                time.sleep(0.5)
            _set_storage_status(status="ready", progress=100, stage="Model storage ready",
                                detail=f"All available models are ready in {MODEL_DIR}.", error="")
            return

        if mode != "wsl" or os.name != "nt":
            raise ValueError("Choose WSL or Windows model storage.")
        distro = os.environ.get("RVE_WSL_DISTRO", "Ubuntai")
        _set_storage_status(status="starting", mode=mode, progress=0, stage=f"Starting WSL · {distro}",
                            detail="Starting the selected WSL distribution if it is stopped.", error="")
        wsl = shutil.which("wsl.exe") or "wsl.exe"
        common = [wsl, "--distribution", distro, "--exec"]
        hidden = hidden_subprocess_kwargs()
        # Running a harmless command starts WSL when it is not already running.
        home_result = subprocess.run(common + ["bash", "-lc", "printf %s \"$HOME\""], capture_output=True,
                                     text=True, encoding="utf-8", errors="replace", timeout=180, **hidden)
        if home_result.returncode:
            raise RuntimeError((home_result.stderr or home_result.stdout or f"Could not start WSL distro {distro}.").strip())
        linux_home = home_result.stdout.strip().replace("\r", "")
        if not linux_home.startswith("/") or ".." in Path(linux_home).parts:
            raise RuntimeError(f"WSL returned an invalid home directory: {linux_home!r}")
        windows_home = linux_home.replace("/", "\\")
        MODEL_DIR = Path(rf"\\wsl.localhost\{distro}{windows_home}\.local\share\REAL-Video-Enhancer\models")
        _set_storage_status(stage="Installing the WSL model library", progress=2,
                            detail=f"WSL is running. Installing weights for {distro}.")
        script = ROOT / "install-all-models-wsl.sh"
        if not script.is_file():
            raise FileNotFoundError("The packaged WSL model installer is missing.")
        path_result = subprocess.run(common + ["wslpath", "-a", str(script)], capture_output=True, text=True,
                                     encoding="utf-8", errors="replace", timeout=60, **hidden)
        if path_result.returncode:
            raise RuntimeError((path_result.stderr or "Could not map the model installer into WSL.").strip())
        script_in_wsl = path_result.stdout.strip().replace("\r", "")
        process = subprocess.Popen(common + ["bash", script_in_wsl], stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace",
                                   bufsize=1, **hidden)
        global storage_process
        with storage_process_lock:
            storage_process = process
        if cancel_event.is_set() and process.poll() is None:
            process.terminate()
        with storage_lock:
            storage_job["status"] = "installing"
        try:
            for line in process.stdout or []:
                detail = line.strip()
                if detail:
                    progress = storage_job.get("progress", 2)
                    if detail.startswith("[") and "/" in detail:
                        try:
                            current, total = detail[1:].split("]", 1)[0].split("/", 1)
                            progress = 2 + round(96 * int(current) / max(1, int(total)))
                        except ValueError:
                            pass
                    _set_storage_status(progress=progress, detail=detail)
            code = process.wait()
        finally:
            with storage_process_lock:
                if storage_process is process:
                    storage_process = None
        if cancel_event.is_set():
            _set_storage_status(status="cancelled", stage="Setup cancelled", detail="WSL model setup was cancelled.", error="")
            cancel_event.clear()
            return
        if code:
            raise RuntimeError(f"WSL model installation exited with code {code}. See the setup log for details.")
        if not MODEL_DIR.is_dir():
            raise RuntimeError(f"Windows could not access the WSL model folder: {MODEL_DIR}")
        _set_storage_status(status="ready", progress=100, stage="Model storage ready",
                            detail=f"All available models are ready in WSL · {MODEL_DIR}.", error="")
    except Exception as exc:
        _set_storage_status(status="cancelled" if isinstance(exc, InterruptedError) else "error", stage="Setup cancelled" if isinstance(exc, InterruptedError) else "Model storage setup failed", error="" if isinstance(exc, InterruptedError) else str(exc), detail=str(exc))
        cancel_event.clear()


def begin_model_storage_setup(mode):
    mode = str(mode or "").strip().lower()
    if mode not in ("wsl", "windows"):
        raise ValueError("Model storage must be either 'wsl' or 'windows'.")
    with storage_lock:
        if storage_job.get("status") in ("starting", "installing"):
            return storage_snapshot()
        cancel_event.clear()
        storage_job.update(status="starting", mode=mode, progress=0, stage="Preparing model storage", detail="Setup queued.", error="")
    threading.Thread(target=prepare_model_storage, args=(mode,), name="rve-model-storage", daemon=True).start()
    return storage_snapshot()


def available_backends_from_probe(opencv_available, installed_backends):
    """Keep CPU PyTorch available even when the detected CUDA arch is unsupported."""
    if not opencv_available:
        return []
    return [name for name in ("pytorch", "ncnn", "tensorrt") if name in installed_backends]


def selected_backend_is_available(capabilities, backend):
    selected = str(backend or "pytorch").strip().lower()
    if selected == "auto":
        selected = "pytorch"
    return bool(capabilities.get("opencv_available")) and selected in capabilities.get("available_backends", [])


def python_capabilities(refresh=False):
    """Probe the managed runtime once and reuse the result for render requests.

    A clean Python 3.12 environment can spend tens of seconds importing CUDA
    PyTorch on its first launch. Rechecking it for every render was turning a
    valid install into a false "backend unavailable" result.
    """
    with capability_cache_lock:
        cached = capability_cache.get("value")
        if cached is not None and not refresh and time.time() - capability_cache.get("checked_at", 0) < 300:
            return dict(cached)
    ffmpeg_path = find_ffmpeg_path() or str(APP_DATA / "bin" / ("ffmpeg.exe" if os.name == "nt" else "ffmpeg"))
    result = {"python": PYTHON or None, "python_available": bool(PYTHON and (Path(PYTHON).exists() or shutil.which(PYTHON))),
              "backend_script_available": BACKEND.is_file(), "ffmpeg_path": str(ffmpeg_path),
              "ffmpeg_available": bool(Path(ffmpeg_path).is_file()),
              "available_backends": [], "runtime_root": str(APP_DATA), "working_directory": str(APP_DATA),
              "backend_script_path": str(BACKEND), "python_runtime": PYTHON, "python_target_path": PYTHON,
              "temp_directory": str(APP_DATA / "temp"), "version": APP_VERSION, "os": f"{platform.system()} {platform.release()} {platform.machine()}",
              "cpu": platform.processor() or platform.machine(), "ram": "Unavailable", "available_disk_gb": None,
              "recommended_backend_label": "Install the project runtime dependencies", "recommended_backend_reason": "The API is connected; render support depends on PyTorch/OpenCV and model weights."}
    try:
        if not PYTHON:
            raise FileNotFoundError("The portable RVE Python runtime is not installed yet.")
        probe_code = """
from importlib import metadata
distributions = {dist.metadata.get('Name', '').lower().replace('_', '-') for dist in metadata.distributions()}
ncnn_packages = {'rife-ncnn-vulkan-python-tntwise', 'upscale-ncnn-py', 'ncnn'}
tensorrt_packages = {'tensorrt', 'tensorrt-cu12-bindings', 'tensorrt-cu12-libs'}
try:
    import cv2
    opencv = True
except Exception:
    opencv = False
try:
    import torch
    import torch.nn.functional
    cuda = torch.cuda.is_available()
    cap = torch.cuda.get_device_capability(0) if cuda else None
    arch = ('sm_' + ''.join(map(str, cap))) if cap else ''
    supported = (not cuda) or arch in torch.cuda.get_arch_list()
    device = torch.cuda.get_device_name(0) if cuda else 'CPU'
    version = torch.__version__
    installed_pytorch = True
except Exception:
    cuda, arch, supported = False, '', True
    device, version, installed_pytorch = 'Unavailable', 'Unavailable', False
try:
    import rife_ncnn_vulkan_python
    installed_ncnn = ncnn_packages.issubset(distributions)
except Exception:
    installed_ncnn = False
try:
    import tensorrt
    installed_tensorrt = tensorrt_packages.issubset(distributions)
except Exception:
    installed_tensorrt = False
print('opencv=' + str(opencv))
print('torch=' + version)
print('device=' + device)
print('cuda_arch=' + arch)
print('cuda_supported=' + str(supported))
print('installed_pytorch=' + str(installed_pytorch))
print('installed_ncnn=' + str(installed_ncnn))
print('installed_tensorrt=' + str(installed_tensorrt))
"""
        probe = subprocess.run([PYTHON, "-c", probe_code], capture_output=True, text=True, timeout=120, **hidden_subprocess_kwargs())
        result["opencv_available"] = probe.returncode == 0 and "opencv=True" in probe.stdout
        if probe.returncode == 0:
            lines = dict(line.split("=", 1) for line in probe.stdout.splitlines() if "=" in line)
            installed_backends = [name for name in ("pytorch", "ncnn", "tensorrt")
                                  if lines.get(f"installed_{name}", "False").lower() == "true"]
            result["backend_management"] = {
                "installed_backends": installed_backends,
                "torch_versions": ["2.9.0", "2.8.0", "2.7.0", "2.6.0"],
                "pytorch_backends": [
                    {"value": "cuda", "label": "CUDA"},
                    {"value": "xpu", "label": "XPU"},
                    {"value": "cpu", "label": "CPU"},
                ],
                "directml_supported": False,
                "tensorrt_supported": True,
            }
            result["available_backends"] = available_backends_from_probe(
                result["opencv_available"], installed_backends
            )
            if "pytorch" in result["available_backends"] and lines.get("cuda_supported", "True").lower() == "false":
                result["backend_probe_output"] = f"Installed PyTorch does not include kernels for {lines.get('cuda_arch', 'this GPU')}. Install the recommended CUDA 12.8 backend."
                result["recommended_backend_label"] = f"PyTorch CPU available · CUDA 12.8 recommended for {lines.get('device', 'this GPU')}"
            elif result["available_backends"]:
                result["recommended_backend_label"] = f"{', '.join(result['available_backends'])} · {lines.get('device', 'CPU')}"
        else:
            result["backend_probe_output"] = probe.stderr.strip()[-1200:]
            result["backend_management"] = {
                "installed_backends": [], "torch_versions": ["2.9.0", "2.8.0", "2.7.0", "2.6.0"],
                "pytorch_backends": [{"value": "cuda", "label": "CUDA"}, {"value": "cpu", "label": "CPU"}],
                "directml_supported": False, "tensorrt_supported": True,
            }
    except Exception as exc:
        result["opencv_available"] = False
        result["backend_probe_output"] = str(exc)
        result["backend_management"] = {
            "installed_backends": [], "torch_versions": ["2.9.0", "2.8.0", "2.7.0", "2.6.0"],
            "pytorch_backends": [{"value": "cuda", "label": "CUDA"}, {"value": "cpu", "label": "CPU"}],
            "directml_supported": False, "tensorrt_supported": True,
        }
    try:
        import psutil
        result["ram"] = f"{psutil.virtual_memory().total / (1024 ** 3):.1f} GB"
        result["available_disk_gb"] = round(shutil.disk_usage(APP_DATA).free / (1024 ** 3), 2)
    except Exception:
        pass
    with capability_cache_lock:
        capability_cache.update(checked_at=time.time(), value=dict(result))
    return result


def find_animescripter():
    candidates = [os.environ.get("ANIMESCRIPTER_EXE"), shutil.which("theanimescripter"), shutil.which("tas"), shutil.which("AnimeScripter")]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return str(Path(candidate).resolve())
    home = Path(os.environ.get("ANIMESCRIPTER_HOME", "")) if os.environ.get("ANIMESCRIPTER_HOME") else None
    roots = [APP_DATA / "integrations",
             APP_DATA / "integrations" / "TheAnimeScripter",
            # Early RVE builds passed `-AddToPath:$false` to PowerShell's
            # -File argument list. Windows PowerShell treated `$false` as the
            # positional InstallPath; recognize that existing portable copy
             # so an upgrade does not download it a second time.
             APP_DATA / "integrations" / "$false",
             Path(os.environ.get("LOCALAPPDATA", "")) / "TheAnimeScripter",
             Path(os.environ.get("LOCALAPPDATA", "")) / "Programs" / "TheAnimeScripter",
             Path(os.environ.get("LOCALAPPDATA", "")) / "Programs" / "The Anime Scripter",
             Path(os.environ.get("PROGRAMFILES", "C:/Program Files")) / "TheAnimeScripter"]
    if home:
        roots.insert(0, home)
    for root in roots:
        # Current upstream Windows installs provide command shims in the
        # selected working directory. Older portable releases expose main.py
        # or an exe inside TheAnimeScripter/TAS-Portable.
        for candidate in (root / "tas.cmd", root / "TheAnimeScripter.cmd",
                          root / "tas.exe", root / "TheAnimeScripter.exe",
                          root / "AnimeScripter.exe", root / "main.py",
                          root / "TAS-Portable" / "main.py",
                          root / "TheAnimeScripter" / "tas.cmd",
                          root / "TheAnimeScripter" / "TheAnimeScripter.cmd",
                          root / "TheAnimeScripter" / "TAS-Portable" / "main.py"):
            if candidate.is_file():
                return str(candidate.resolve())
    return None


def animescripter_python(script):
    script = Path(script)
    executable = "python.exe" if os.name == "nt" else "python"
    for candidate in (script.parent / executable, script.parent / "python" / executable,
                      script.parent / "python" / "python" / executable):
        if candidate.is_file():
            return str(candidate.resolve())
    return os.environ.get("RVE_ANIMESCRIPTER_PYTHON") or PYTHON


def animescripter_invocation(executable, args):
    suffix = Path(executable).suffix.lower()
    if suffix == ".py":
        runtime = animescripter_python(executable)
        if not runtime:
            raise RuntimeError("AnimeScripter's Python runtime was not found.")
        return [runtime, executable, *args]
    if suffix in (".cmd", ".bat"):
        shell = os.environ.get("COMSPEC") or "cmd.exe"
        command = subprocess.list2cmdline([executable, *args])
        return [shell, "/d", "/s", "/c", command]
    return [executable, *args]


def animescripter_status():
    exe = find_animescripter()
    if not exe:
        return {"available": False, "executable": None, "detail": "AnimeScripter integration is unfinished. Its code is preserved for later work."}
    try:
        command = animescripter_invocation(exe, ["--help"])
        probe = subprocess.run(command, capture_output=True, text=True, timeout=15, check=False, **hidden_subprocess_kwargs())
        return {"available": True, "executable": exe, "detail": "AnimeScripter integration is unfinished. CLI detected; native app integration is planned for later.", "help": (probe.stdout + probe.stderr)[:10000]}
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {"available": True, "executable": exe, "detail": f"AnimeScripter integration is unfinished. Executable found; capability probe failed: {exc}"}


def unavailable(message, status=501):
    return {"error": message, "available": False}, status


def set_install_state(**updates):
    with install_lock:
        install_state.update(updates)


def install_log(message):
    with install_lock:
        lines = install_state.setdefault("output", [])
        lines.append(str(message).rstrip())
        install_state["output"] = lines[-500:]
        install_state["detail"] = str(message).rstrip()[-500:]


def managed_python_path():
    if os.name == "nt":
        return LOCAL_DATA / "python" / "python" / "Scripts" / "python.exe"
    return LOCAL_DATA / "python" / "python" / "bin" / "python3"


def managed_uv_path():
    return APP_DATA / "bin" / ("uv.exe" if os.name == "nt" else "uv")


def ensure_uv():
    uv = managed_uv_path()
    if uv.is_file():
        return uv
    if os.name == "nt":
        asset = "uv-x86_64-pc-windows-msvc.zip" if platform.machine().lower() in ("amd64", "x86_64") else "uv-aarch64-pc-windows-msvc.zip"
        expected = UV_SHA256_X64 if "x86_64" in asset else None
        if not expected:
            raise RuntimeError("Automatic uv setup currently supports x64 Windows only.")
    elif sys.platform.startswith("linux"):
        asset = "uv-x86_64-unknown-linux-gnu.tar.gz" if platform.machine().lower() in ("amd64", "x86_64") else "uv-aarch64-unknown-linux-gnu.tar.gz"
        expected = None
    else:
        raise RuntimeError("Automatic uv setup is not available for this platform.")
    version = UV_VERSION
    archive = APP_DATA / "temp" / asset
    url = f"https://github.com/astral-sh/uv/releases/download/{version}/{asset}"
    set_install_state(stage="Downloading uv", progress=3, detail="Preparing the app-local Python package manager.")
    download_runtime_file(url, archive)
    if expected:
        import hashlib
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        if digest != expected:
            archive.unlink(missing_ok=True)
            raise RuntimeError("The uv download did not match Astral’s published SHA-256 checksum.")
    uv.parent.mkdir(parents=True, exist_ok=True)
    if asset.endswith(".zip"):
        with zipfile.ZipFile(archive) as bundle:
            member = next((item for item in bundle.namelist() if item == "uv.exe" or item.endswith("/uv.exe")), None)
            if not member:
                raise RuntimeError("The uv archive did not contain uv.exe.")
            uv.write_bytes(bundle.read(member))
    else:
        with tarfile.open(archive, "r:gz") as bundle:
            member = next((item for item in bundle.getmembers() if item.name.endswith("/uv") and item.isfile()), None)
            if not member:
                raise RuntimeError("The uv archive did not contain its executable.")
            uv.write_bytes(bundle.extractfile(member).read())
        uv.chmod(0o755)
    archive.unlink(missing_ok=True)
    return uv


def download_runtime_file(url, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    partial = target.with_suffix(target.suffix + ".part")
    received = 0
    with urllib.request.urlopen(url, timeout=45) as response, partial.open("wb") as stream:
        total = int(response.headers.get("Content-Length", "0") or 0)
        while True:
            if cancel_event.is_set():
                partial.unlink(missing_ok=True)
                raise InterruptedError("Setup cancelled.")
            block = response.read(1024 * 1024)
            if not block:
                break
            stream.write(block)
            received += len(block)
            if total:
                set_install_state(progress=min(35, round(received * 35 / total)))
    partial.replace(target)


def ensure_managed_python():
    global PYTHON
    python = managed_python_path()
    if python.is_file():
        PYTHON = str(python)
        os.environ["PYTHONNOUSERSITE"] = "1"
        return
    legacy_python = LOCAL_DATA / "python" / "python" / "python.exe"
    if os.name == "nt" and legacy_python.is_file():
        PYTHON = str(legacy_python)
        os.environ["PYTHONNOUSERSITE"] = "1"
        return
    uv = ensure_uv()
    env = os.environ.copy()
    env["UV_PYTHON_INSTALL_DIR"] = str(LOCAL_DATA / "python" / "managed")
    set_install_state(stage=f"Installing Python {PYTHON_VERSION} with uv", progress=10)
    run_runtime_command([uv, "python", "install", PYTHON_VERSION, "--managed-python"], f"Installing Python {PYTHON_VERSION}", env=env)
    python.parent.mkdir(parents=True, exist_ok=True)
    run_runtime_command([uv, "venv", "--python", PYTHON_VERSION, "--managed-python", str(python.parent)], "Creating managed Python environment", env=env)
    if not python.is_file():
        raise RuntimeError(f"Python download extracted, but the runtime executable is missing: {python}")
    PYTHON = str(python)
    os.environ["PYTHONNOUSERSITE"] = "1"
    set_install_state(stage="Preparing Python packages", progress=43)


def run_runtime_command(command, stage, progress=None, env=None):
    if progress is not None:
        set_install_state(stage=stage, progress=progress)
    command_env = os.environ.copy()
    if env:
        command_env.update(env)
    command_env["PIP_DISABLE_PIP_VERSION_CHECK"] = "1"
    command_env["PYTHONNOUSERSITE"] = "1"
    command_env["TMP"] = str(APP_DATA / "temp")
    command_env["TEMP"] = str(APP_DATA / "temp")
    command_env["UV_CACHE_DIR"] = str(LOCAL_DATA / "uv-cache")
    Path(command_env["TEMP"]).mkdir(parents=True, exist_ok=True)
    command = [str(item) for item in command]
    install_log("$ " + subprocess.list2cmdline([str(x) for x in command]))
    proc = subprocess.Popen(command, cwd=str(APP_DATA), env=command_env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, encoding="utf-8", errors="replace", bufsize=1,
                            **hidden_subprocess_kwargs())
    global runtime_process
    with runtime_process_lock:
        runtime_process = proc
    try:
        for line in proc.stdout:
            if cancel_event.is_set() and proc.poll() is None:
                proc.terminate()
            install_log(line)
    finally:
        code = proc.wait()
        with runtime_process_lock:
            runtime_process = None
    if cancel_event.is_set():
        raise InterruptedError("Runtime setup cancelled.")
    if code:
        raise RuntimeError(f"{stage} failed with exit code {code}. See the download manager output for details.")


RUNTIME_COMMON = ["testresources==2.0.1", "requests==2.32.3", "opencv-python-headless==4.11.0.86", "pypresence==4.3.0",
                  "scenedetect==0.6.5.2", "numpy==2.2.2", "sympy", "tqdm==4.67.1", "typing_extensions==4.12.2",
                  "packaging==24.2", "mpmath==1.3.0", "pillow==11.1.0"]


def backend_packages(backend, payload=None):
    backend = str(backend or "pytorch").lower()
    payload = payload or {}
    if backend in ("torch", "pytorch", "tensorrt"):
        version = str(payload.get("torch_version") or "2.7.0")
        if version == "2.6": version = "2.6.0"
        if version not in ("2.6.0", "2.7.0", "2.8.0", "2.9.0"):
            raise ValueError("Unsupported PyTorch version. Choose 2.6.0, 2.7.0, 2.8.0, or 2.9.0.")
        flavor = str(payload.get("pytorch_backend") or "cuda").lower().replace("+", "")
        if flavor in ("cuda", "gpu"):
            flavor = {"2.6.0": "cu118", "2.7.0": "cu128", "2.8.0": "cu129", "2.9.0": "cu130"}[version]
        allowed = {"cpu", "mps", "cu118", "cu126", "cu128", "cu129", "cu130", "rocm64", "xpu"}
        if flavor not in allowed:
            raise ValueError("Unsupported PyTorch runtime type.")
        compatible_flavors = {"2.6.0": {"cu118", "cu124", "cu126"}, "2.7.0": {"cu118", "cu126", "cu128"}, "2.8.0": {"cu126", "cu128", "cu129"}, "2.9.0": {"cu126", "cu128", "cu130"}}[version]
        if flavor.startswith("cu") and flavor not in compatible_flavors:
            raise ValueError(f"PyTorch {version} is not available with CUDA {flavor[2:]}; choose a listed CUDA version or CPU.")
        suffix = "" if flavor in ("cpu", "mps") else "+" + flavor
        packages = [f"torch=={version}{suffix}", "safetensors==0.5.3", "einops==0.8.1"]
        if flavor.startswith("cu12"):
            packages.append("cupy-cuda12x==13.3.0")
        if backend == "tensorrt":
            packages += ["tensorrt==10.12.0.36", "tensorrt_cu12==10.12.0.36", "tensorrt-cu12_libs==10.12.0.36", "tensorrt_cu12_bindings==10.12.0.36"]
        return packages
    if backend == "ncnn":
        return ["rife-ncnn-vulkan-python-tntwise==1.4.5", "upscale_ncnn_py==1.2.0", "ncnn==1.0.20250916", "numpy==2.2.2"]
    raise ValueError(f"Unsupported backend: {backend}")


def setup_runtime_worker(action="recommended", backend="pytorch", payload=None):
    global PYTHON
    try:
        set_install_state(status="running", progress=1, stage="Preparing runtime", detail="Starting managed runtime setup.", output=[])
        APP_DATA.mkdir(parents=True, exist_ok=True)
        (APP_DATA / "bin").mkdir(parents=True, exist_ok=True)
        (APP_DATA / "temp").mkdir(parents=True, exist_ok=True)
        if action == "recommended":
            ensure_managed_python()
            uv = ensure_uv()
            common = [str(uv), "pip", "install", "--python", PYTHON, "--no-progress", "--extra-index-url", "https://download.pytorch.org/whl/", "--extra-index-url", "https://download.pytorch.org/whl/test/"]
            run_runtime_command(common + RUNTIME_COMMON, "Installing RVE runtime dependencies", 46)
            apple_silicon = sys.platform == "darwin" and platform.machine().lower() in ("arm64", "aarch64")
            use_cuda = bool(shutil.which("nvidia-smi"))
            backend = "torch" if use_cuda or apple_silicon else "ncnn"
            torch_flavor = "cu128" if use_cuda else "mps" if apple_silicon else "cpu"
            packages = backend_packages(backend, {"torch_version": "2.7.0", "pytorch_backend": torch_flavor})
            run_runtime_command(common + packages, f"Installing {backend} backend", 74)
            if backend == "torch":
                tv_suffix = "+cu128" if torch_flavor.startswith("cu") else ""
                run_runtime_command([str(uv), "pip", "install", "--python", PYTHON, "--no-progress", "--no-deps", "--extra-index-url", "https://download.pytorch.org/whl/", f"torchvision==0.22.0{tv_suffix}"], "Installing compatible torchvision", 86)
            ffmpeg = APP_DATA / "bin" / ("ffmpeg.exe" if os.name == "nt" else "ffmpeg")
            if not ffmpeg.exists():
                if os.name == "nt": asset = "ffmpeg.exe" if platform.machine() in ("AMD64", "x86_64") else "ffmpeg-windows-arm64.exe"
                elif sys.platform == "darwin": asset = "ffmpeg-macos-bin" if platform.machine() == "x86_64" else "ffmpeg-macos-arm"
                else: asset = "ffmpeg" if platform.machine() in ("AMD64", "x86_64") else "ffmpeg-linux-arm64"
                set_install_state(stage="Downloading FFmpeg", progress=92)
                download_runtime_file("https://github.com/TNTwise/real-video-enhancer-models/releases/download/models/" + asset, ffmpeg)
            tas_ready = bool(find_animescripter())
            detail = f"Recommended runtime and {backend} backend installed. "
            detail += "AnimeScripter CLI is already available." if tas_ready else "AnimeScripter remains optional and is not installed with the Windows runtime."
            set_install_state(status="completed", progress=100, stage="Runtime ready", detail=detail)
        elif action == "animescripter":
            if find_animescripter():
                set_install_state(status="completed", progress=100, stage="AnimeScripter ready", detail="AnimeScripter CLI is already installed.")
            else:
                install_animescripter_cli()
                detected = find_animescripter()
                if not detected:
                    raise RuntimeError("The official installer completed, but its CLI entry point was not found.")
                set_install_state(status="completed", progress=100, stage="AnimeScripter ready", detail=f"AnimeScripter CLI installed at {detected}.")
        else:
            ensure_managed_python()
            packages = backend_packages(backend, payload)
            uv = ensure_uv()
            pip = [str(uv), "pip"]
            if action == "install":
                command = pip + ["install", "--python", PYTHON, "--no-progress", "--extra-index-url", "https://download.pytorch.org/whl/", "--extra-index-url", "https://download.pytorch.org/whl/test/"] + packages
            else:
                names = [str(pkg).split("==", 1)[0] for pkg in packages]
                if backend in ("torch", "pytorch", "tensorrt"):
                    names.append("torchvision")
                command = pip + ["uninstall", "--python", PYTHON, "-y"] + names
            run_runtime_command(command, f"{action.capitalize()} {backend} backend", 20)
            if action == "install" and backend in ("torch", "pytorch", "tensorrt"):
                version = str(payload.get("torch_version") or "2.7.0")
                version = {"2.6": "2.6.0"}.get(version, version)
                tv_version = {"2.6.0": "0.21.0", "2.7.0": "0.22.0", "2.8.0": "0.23.0", "2.9.0": "0.24.0"}.get(version)
                flavor = str(payload.get("pytorch_backend") or "cuda").lower().replace("+", "")
                flavor = {"cuda": {"2.6.0": "cu118", "2.7.0": "cu128", "2.8.0": "cu129", "2.9.0": "cu130"}.get(version, "cu128")}.get(flavor, flavor)
                suffix = "" if flavor == "cpu" else "+" + flavor
                if tv_version:
                    run_runtime_command(pip + ["install", "--python", PYTHON, "--no-progress", "--no-deps", "--extra-index-url", "https://download.pytorch.org/whl/", f"torchvision=={tv_version}{suffix}"], "Installing compatible torchvision", 80)
            set_install_state(status="completed", progress=100, stage="Backend operation complete", detail=f"{backend} {action} completed. Refresh backend status to see the result.")
    except Exception as exc:
        install_log(f"ERROR: {exc}")
        set_install_state(status="cancelled" if isinstance(exc, InterruptedError) else "error", stage="Setup cancelled" if isinstance(exc, InterruptedError) else "Runtime setup failed", detail=str(exc))
        cancel_event.clear()


def install_animescripter_cli():
    if os.name != "nt":
        raise RuntimeError("The integrated AnimeScripter installer is currently available on Windows.")
    powershell = shutil.which("powershell.exe") or shutil.which("powershell")
    if not powershell:
        raise RuntimeError("Windows PowerShell was not found; AnimeScripter cannot be installed automatically.")
    install_root = APP_DATA / "integrations"
    script_path = APP_DATA / "temp" / "tas-install.ps1"
    install_root.mkdir(parents=True, exist_ok=True)
    script_path.parent.mkdir(parents=True, exist_ok=True)
    set_install_state(stage="Downloading AnimeScripter installer", progress=93, detail="Fetching the official Windows CLI installer from tas.nevermindnilas.dev.")
    request = urllib.request.Request("https://tas.nevermindnilas.dev/install.ps1", headers={"User-Agent": f"REAL-Video-Enhancer/{APP_VERSION}"})
    with urllib.request.urlopen(request, timeout=45) as response:
        script = response.read(1024 * 1024 + 1)
    if not script or len(script) > 1024 * 1024:
        raise RuntimeError("The official AnimeScripter installer response was empty or unexpectedly large.")
    script_path.write_bytes(script)
    set_install_state(stage="Installing AnimeScripter CLI", progress=95, detail="Installing the official portable CLI into the RVE application data folder.")
    install_log("Downloading and running the official TAS Windows CLI installer (PATH registration declined).")
    # The upstream script's default interactive flow asks whether to edit
    # PATH. RVE invokes its entry point by absolute path, so explicitly keep
    # PATH untouched and let this run headlessly during first-run setup.
    # Upstream exposes TAS_INSTALL_ADD_TO_PATH for unattended setup. Passing
    # `-AddToPath:$false` after PowerShell's -File can be interpreted as the
    # positional InstallPath on Windows PowerShell, creating a literal `$false`
    # directory. Use the documented environment override and let the script
    # install into its normal `<cwd>\TheAnimeScripter` location.
    installer_env = os.environ.copy()
    installer_env["TAS_INSTALL_ADD_TO_PATH"] = "0"
    proc = subprocess.Popen([powershell, "-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass",
                             "-File", str(script_path)], cwd=str(install_root), env=installer_env,
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, encoding="utf-8", errors="replace", bufsize=1,
                            **hidden_subprocess_kwargs())
    global runtime_process
    with runtime_process_lock:
        runtime_process = proc
    try:
        if proc.stdin:
            proc.stdin.write("N\r\n")
            proc.stdin.flush()
        if proc.stdout:
            for line in proc.stdout:
                install_log(line)
        code = proc.wait()
    finally:
        if proc.stdin:
            proc.stdin.close()
        with runtime_process_lock:
            runtime_process = None
    script_path.unlink(missing_ok=True)
    if cancel_event.is_set():
        raise InterruptedError("AnimeScripter setup cancelled.")
    if code:
        raise RuntimeError(f"The official AnimeScripter CLI installer failed with exit code {code}.")


def begin_runtime_operation(action="recommended", backend="pytorch", payload=None):
    with install_lock:
        if install_state.get("status") == "running":
            raise ValueError("A runtime installation is already in progress.")
        if download_job.get("active") or storage_job.get("status") in ("starting", "installing"):
            raise ValueError("Wait for model or storage setup to finish before changing the runtime.")
        cancel_event.clear()
        install_state.update(status="running", progress=0, stage="Queued", detail="Runtime operation queued.", output=[])
    threading.Thread(target=setup_runtime_worker, args=(action, backend, payload or {}), daemon=True).start()
    return {**install_state}


def status_snapshot():
    with state_lock:
        proc = render.get("process")
        if proc is not None and proc.poll() is not None and render["status"] in ("running", "paused", "stopping"):
            render["status"] = "stopped" if render.get("status") == "stopping" else "completed" if proc.returncode == 0 else "error"
            render["paused"] = False
            if proc.returncode:
                render["log_lines"].append(f"Renderer exited with code {proc.returncode}.")
        with download_lock:
            model_download_status = {"download_active": download_job["active"], "download_progress": download_job["progress"],
                                     "download_label": download_job["label"], "download_detail": download_job["detail"],
                                     "download_stage": download_job.get("stage", ""), "download_error": download_job.get("error", "")}
        preview_available = bool(
            render.get("preview_enabled")
            and render.get("engine") == "rve"
            and (
                (render.get("preview_memory") is not None and render.get("status") in ("running", "paused"))
                or render.get("preview_jpeg") is not None
            )
        )
        return {k: v for k, v in render.items() if k not in ("process", "pause_memory", "preview_memory", "preview_jpeg")} | model_download_status | {"preview": {"available": preview_available, "url": "/api/render/preview_frame" if preview_available else None}}


def read_preview_jpeg(memory):
    """Read a stable JPEG packet from renderer shared memory, or return None."""
    if memory is None:
        return None
    try:
        first_sequence, width, height, length = struct.unpack_from("<IIII", memory.buf, 0)
        if first_sequence == 0 or first_sequence & 1 or not width or not height or not length or length > len(memory.buf) - 16:
            return None
        content = bytes(memory.buf[16:16 + length])
        second_sequence = struct.unpack_from("<I", memory.buf, 0)[0]
        if first_sequence != second_sequence or second_sequence & 1:
            return None
        if not content.startswith(b"\xff\xd8"):
            return None
        return content
    except (BufferError, OSError, struct.error, ValueError):
        return None


def resolve_model(value, backend="pytorch", category=None):
    if not value:
        return None
    candidate = Path(value).expanduser()
    if candidate.is_file():
        return str(candidate.resolve())
    _, catalog = model_catalog(backend)
    categories = (category,) if category else catalog.keys()
    for model_category in categories:
        spec = catalog.get(model_category, {}).get(str(value))
        if spec:
            exact = find_model_file(spec[0])
            if exact:
                return str(exact.resolve())
    # The UI's friendly model label is not a checkpoint path. Only accept a
    # unique on-disk filename match; never pass a label as if it were a model.
    search_roots = [MODEL_DIR, custom_models_dir(), Path(os.environ.get("LOCALAPPDATA", "")) / "REAL Video Enhancer" / "models"]
    token = "".join(ch.lower() for ch in str(value) if ch.isalnum())
    matches = []
    for folder in search_roots:
        if folder.is_dir():
            for item in folder.rglob("*"):
                if item.is_file() and token and token in "".join(ch.lower() for ch in item.stem if ch.isalnum()):
                    matches.append(item)
                    if len(matches) > 1:
                        return None
    return str(matches[0].resolve()) if matches else None


def model_status(labels, backend="pytorch"):
    _, catalog = model_catalog(backend)
    output = {}
    for category, names in (labels or {}).items():
        output[category] = {}
        for label in names or []:
            spec = catalog.get(category, {}).get(label)
            if spec:
                output[category][label] = bool(find_model_file(spec[0]))
            else:
                custom = custom_models_dir() / str(label)
                output[category][label] = custom.exists() if custom.exists() else False
    return output


VIDEO_EXTENSIONS = {".mp4", ".mkv", ".mov", ".avi", ".webm", ".m4v", ".wmv", ".flv", ".ts", ".mpg", ".mpeg", ".m2ts", ".mts", ".vob", ".ogv", ".3gp"}


def choose_path(kind, initial=None):
    """Show a native picker that works in the packaged browser launcher.

    Tkinter is intentionally omitted from the frozen build. Windows Forms
    provides the same local file/folder paths that the former Qt dialogs did.
    """
    if os.name == "nt":
        powershell = shutil.which("powershell.exe") or shutil.which("powershell")
        if not powershell:
            raise RuntimeError("Windows PowerShell is required to open a file or folder picker.")
        is_model = kind in ("model", "models")
        is_folder = kind in ("folder", "batch_folder")
        multiple = kind in ("models", "inputs")
        config = {
            "kind": kind,
            "initial": str(initial or (custom_models_dir() if is_model else Path.home())),
            "title": {
                "input": "Select a video",
                "inputs": "Select a folder containing videos",
                "batch_folder": "Select a folder containing videos",
                "folder": "Select an output folder",
                "model": "Select a model file",
                "models": "Select model files",
            }.get(kind, "Select a folder"),
            "filter": "Model files (*.pth;*.pt;*.onnx;*.safetensors;*.engine;*.bin;*.param;*.ckpt)|*.pth;*.pt;*.onnx;*.safetensors;*.engine;*.bin;*.param;*.ckpt|All files (*.*)|*.*" if is_model else "Video files (*.mp4;*.mkv;*.mov;*.avi;*.webm;*.m4v;*.wmv;*.flv;*.ts;*.mpg;*.mpeg;*.m2ts;*.mts;*.vob;*.ogv;*.3gp)|*.mp4;*.mkv;*.mov;*.avi;*.webm;*.m4v;*.wmv;*.flv;*.ts;*.mpg;*.mpeg;*.m2ts;*.mts;*.vob;*.ogv;*.3gp|All files (*.*)|*.*",
            "folder": is_folder,
            "multiple": multiple,
        }
        encoded = __import__("base64").b64encode(json.dumps(config).encode("utf-8")).decode("ascii")
        script = r'''
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Windows.Forms
$cfg = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('__CONFIG__')) | ConvertFrom-Json
$owner = New-Object System.Windows.Forms.Form
$owner.Text = 'REAL Video Enhancer picker'
$owner.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::FixedToolWindow
$owner.ShowInTaskbar = $false
$owner.StartPosition = [System.Windows.Forms.FormStartPosition]::CenterScreen
$owner.Size = New-Object System.Drawing.Size(1, 1)
$owner.Opacity = 0
$owner.TopMost = $true
$owner.Show()
try {
  if ($cfg.folder) {
    $dialog = New-Object System.Windows.Forms.FolderBrowserDialog
    $dialog.Description = [string]$cfg.title
    $dialog.ShowNewFolderButton = $true
    if (Test-Path -LiteralPath ([string]$cfg.initial) -PathType Container) { $dialog.SelectedPath = [string]$cfg.initial }
    $result = $dialog.ShowDialog($owner)
    $value = if ($result -eq [System.Windows.Forms.DialogResult]::OK) { [string]$dialog.SelectedPath } else { '' }
    $response = @{ path = $value; paths = @() }
  } else {
    $dialog = New-Object System.Windows.Forms.OpenFileDialog
    $dialog.Title = [string]$cfg.title
    $dialog.Filter = [string]$cfg.filter
    $dialog.Multiselect = [bool]$cfg.multiple
    $dialog.CheckFileExists = $true
    $dialog.CheckPathExists = $true
    if (Test-Path -LiteralPath ([string]$cfg.initial) -PathType Container) { $dialog.InitialDirectory = [string]$cfg.initial }
    $result = $dialog.ShowDialog($owner)
    $value = if ($result -eq [System.Windows.Forms.DialogResult]::OK) { [string]$dialog.FileName } else { '' }
    $selected = if ($result -eq [System.Windows.Forms.DialogResult]::OK) { @($dialog.FileNames) } else { @() }
    $response = @{ path = $value; paths = $selected }
  }
  [Console]::WriteLine((ConvertTo-Json -InputObject $response -Compress -Depth 4))
} finally {
  $owner.Close()
  $owner.Dispose()
}
'''.replace("__CONFIG__", encoded)
        try:
            result = subprocess.run(
                [powershell, "-NoLogo", "-NoProfile", "-NonInteractive", "-STA", "-ExecutionPolicy", "Bypass", "-Command", script],
                capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=3600,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
        except subprocess.TimeoutExpired as exc:
            raise RuntimeError("The file picker timed out. Please try selecting the file again.") from exc
        if result.returncode:
            detail = (result.stderr or result.stdout or "Windows file picker failed.").strip()[-1000:]
            raise RuntimeError(detail)
        try:
            selection = json.loads(result.stdout.strip().splitlines()[-1])
        except (json.JSONDecodeError, IndexError) as exc:
            raise RuntimeError("Windows file picker returned an invalid selection.") from exc
        if kind in ("models", "inputs"):
            return [str(item) for item in selection.get("paths", []) if item]
        return str(selection.get("path") or "")

    # Keep a Tk fallback for source builds on platforms where it is available.
    import tkinter
    from tkinter import filedialog
    root = tkinter.Tk()
    root.withdraw()
    try:
        root.attributes("-topmost", True)
        root.update_idletasks()
        if kind == "input":
            return filedialog.askopenfilename(parent=root, title="Select a video", initialdir=initial or str(Path.home()), filetypes=[("Video files", "*.mp4 *.mkv *.mov *.avi *.webm *.m4v *.wmv *.flv *.ts"), ("All files", "*.*")])
        if kind == "inputs":
            return list(filedialog.askopenfilenames(parent=root, title="Select videos", initialdir=initial or str(Path.home()), filetypes=[("Video files", "*.mp4 *.mkv *.mov *.avi *.webm *.m4v *.wmv *.flv *.ts"), ("All files", "*.*")]))
        if kind == "model":
            return filedialog.askopenfilename(parent=root, title="Select model file", initialdir=initial or str(custom_models_dir()), filetypes=[("Model files", "*.pth *.pt *.onnx *.safetensors *.engine *.bin *.param *.ckpt"), ("All files", "*.*")])
        if kind == "models":
            return list(filedialog.askopenfilenames(parent=root, title="Select model files", initialdir=initial or str(custom_models_dir()), filetypes=[("Model files", "*.pth *.pt *.onnx *.safetensors *.engine *.bin *.param *.ckpt"), ("All files", "*.*")]))
        return filedialog.askdirectory(parent=root, title="Select folder", initialdir=initial or str(Path.home()), mustexist=True)
    finally:
        root.destroy()


def list_videos_in_folder(folder):
    root = Path(folder).expanduser().resolve()
    if not root.is_dir():
        return []
    # The original batch picker scanned the selected directory (one level).
    return [str(path.resolve()) for path in sorted(root.iterdir(), key=lambda item: item.name.casefold())
            if path.is_file() and path.suffix.lower() in VIDEO_EXTENSIONS]


def inspect_videos(paths):
    """Validate a selected batch in one managed-Python process."""
    if not paths:
        return [], {}, []
    if not PYTHON or not Path(PYTHON).is_file():
        raise ValueError("The managed Python runtime is unavailable. Install the recommended runtime first.")
    probe_script = r'''import cv2, json, sys
paths = json.loads(sys.stdin.read())
valid, rejected, first_info = [], [], {}
for path in paths:
    cap = None
    try:
        cap = cv2.VideoCapture(path)
        width = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH) or 0)
        height = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT) or 0)
        fps = float(cap.get(cv2.CAP_PROP_FPS) or 0)
        frames = int(cap.get(cv2.CAP_PROP_FRAME_COUNT) or 0)
        ok, _ = cap.read() if cap.isOpened() else (False, None)
        if not ok or width < 1 or height < 1 or frames <= 1:
            rejected.append(path)
            continue
        valid.append(path)
        if not first_info:
            first_info = {"width": width, "height": height, "fps": fps, "frames": frames,
                          "duration_seconds": frames / fps if fps > 0 else 0,
                          "container": path.rsplit('.', 1)[-1].lower() if '.' in path else None}
    except Exception:
        rejected.append(path)
    finally:
        if cap is not None:
            cap.release()
print(json.dumps({"paths": valid, "invalid": rejected, "video_info": first_info}, separators=(',', ':')))
'''
    command = [PYTHON, "-c", probe_script]
    result = subprocess.run(command, input=json.dumps([str(Path(item).resolve()) for item in paths]),
                            capture_output=True, text=True, encoding="utf-8", errors="replace",
                            timeout=min(300, max(45, len(paths) * 2)),
                            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    if result.returncode:
        detail = (result.stderr or result.stdout or "The selected videos could not be read.").strip()[-1200:]
        raise ValueError(detail)
    try:
        payload = json.loads(result.stdout.strip().splitlines()[-1])
        return payload.get("paths") or [], payload.get("video_info") or {}, payload.get("invalid") or []
    except (json.JSONDecodeError, IndexError) as exc:
        raise ValueError("Video validation returned an unreadable response.") from exc


def inspect_video(path, include_frame=False):
    video = Path(path).expanduser()
    if not video.is_file() or not PYTHON or not Path(PYTHON).is_file():
        raise ValueError("The selected video or managed Python runtime is unavailable.")
    probe_script = r'''import base64, cv2, json, sys
path = sys.argv[1]
cap = cv2.VideoCapture(path)
if not cap.isOpened():
    raise RuntimeError("OpenCV could not decode this video file.")
width = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH) or 0)
height = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT) or 0)
fps = float(cap.get(cv2.CAP_PROP_FPS) or 0)
frames = int(cap.get(cv2.CAP_PROP_FRAME_COUNT) or 0)
fourcc_value = int(cap.get(cv2.CAP_PROP_FOURCC) or 0)
codec = ''.join(chr((fourcc_value >> (8 * index)) & 255) for index in range(4)).strip('\x00')
ok, frame = cap.read()
if not ok or width < 1 or height < 1 or frames <= 1:
    cap.release()
    raise RuntimeError("This file does not contain a decodable video with more than one frame.")
result = {"width": width, "height": height, "fps": fps, "frames": frames,
          "duration_seconds": frames / fps if fps > 0 else 0, "codec": codec or None,
          "container": path.rsplit('.', 1)[-1].lower() if '.' in path else None,
          "size_bytes": __import__('os').path.getsize(path)}
if len(sys.argv) > 2 and sys.argv[2] == "frame":
    if ok:
        height, width = frame.shape[:2]
        scale = min(1.0, 1280 / width, 720 / height)
        if scale < 1.0:
            frame = cv2.resize(frame, (round(width * scale), round(height * scale)), interpolation=cv2.INTER_AREA)
        encoded, buffer = cv2.imencode('.jpg', frame, [int(cv2.IMWRITE_JPEG_QUALITY), 82])
        if encoded:
            result["frame"] = base64.b64encode(buffer.tobytes()).decode('ascii')
cap.release()
print(json.dumps(result, separators=(',', ':')))
'''
    command = [PYTHON, "-c", probe_script, str(video.resolve())]
    if include_frame:
        command.append("frame")
    result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", errors="replace",
                            timeout=45, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    if result.returncode:
        error_output = (result.stderr or result.stdout or "The selected video could not be read.").strip()
        detail = next((line.strip() for line in reversed(error_output.splitlines())
                       if line.strip() and not line.lstrip().startswith("File \"")), error_output[-1200:])
        raise ValueError(detail)
    try:
        return json.loads(result.stdout.strip().splitlines()[-1])
    except (json.JSONDecodeError, IndexError) as exc:
        raise ValueError("Video metadata could not be read from the selected file.") from exc


def custom_models_payload():
    folder = custom_models_dir()
    folder.mkdir(parents=True, exist_ok=True)
    models = []
    for item in sorted(folder.iterdir()):
        size = sum(p.stat().st_size for p in item.rglob("*") if p.is_file()) if item.is_dir() else item.stat().st_size
        suffix = item.suffix.lower() if item.is_file() else ""
        is_ncnn = item.is_dir()
        is_torch = suffix in (".pth", ".pt", ".safetensors")
        models.append({"name": item.name, "path": str(item.resolve()), "is_dir": item.is_dir(), "category": "custom", "scale": 1,
                       "size_label": f"{size / (1024 ** 3):.2f} GB" if size >= 1024 ** 3 else f"{size / (1024 ** 2):.1f} MB",
                       "backend_categories": {"pytorch": "custom" if is_torch else None,
                                              "ncnn": "custom" if is_ncnn else None,
                                              "directml": "custom" if suffix == ".onnx" else None,
                                              "tensorrt": "custom" if is_torch or suffix == ".engine" else None}})
    return {"folder": str(folder.resolve()), "models": models}


def import_custom_models(source_paths):
    folder = custom_models_dir()
    folder.mkdir(parents=True, exist_ok=True)
    allowed = {".pth", ".pt", ".onnx", ".safetensors", ".engine", ".bin", ".param", ".ckpt"}
    imported, failures = [], []
    for source in source_paths:
        path = Path(source)
        if not path.is_file() or path.suffix.lower() not in allowed:
            failures.append(f"Skipped unsupported model: {path.name}"); continue
        target = folder / path.name
        if target.exists():
            failures.append(f"Already exists: {path.name}"); continue
        try:
            shutil.copy2(path, target); imported.append(str(target.resolve()))
        except OSError as exc:
            failures.append(f"{path.name}: {exc}")
    return imported, failures


def receive_custom_model_upload(handler, filename):
    """Stream a selected model into the custom collection without Tk/PySide."""
    raw_filename = str(filename or "")
    filename = Path(raw_filename).name
    allowed = {".pth", ".pt", ".onnx", ".safetensors", ".engine", ".bin", ".param", ".ckpt"}
    if not filename or filename in (".", "..") or filename != raw_filename or Path(filename).suffix.lower() not in allowed:
        raise ValueError("Choose a supported model file (.pth, .safetensors, .onnx, .engine, .bin, or .param).")
    try:
        remaining = int(handler.headers.get("Content-Length", "0"))
    except ValueError:
        raise ValueError("The selected model upload has an invalid size.")
    if remaining <= 0 or remaining > 20 * 1024 ** 3:
        raise ValueError("The selected model file is empty or exceeds the 20 GB import limit.")

    root = custom_models_dir().resolve()
    if Path(filename).suffix.lower() in (".bin", ".param"):
        # The legacy RVE importer kept each NCNN .bin/.param pair together in
        # one folder named after the model's .bin file.
        target = root / Path(filename).stem / filename
    else:
        target = root / filename
    target.parent.mkdir(parents=True, exist_ok=True)
    target = target.resolve()
    if target != root and root not in target.parents:
        raise ValueError("The model filename is invalid.")
    if target.exists():
        raise FileExistsError(f"{filename} is already in the custom model collection.")
    partial = target.with_name(f".{target.name}.{uuid.uuid4().hex}.upload")
    try:
        with partial.open("xb") as output:
            while remaining:
                chunk = handler.rfile.read(min(1024 * 1024, remaining))
                if not chunk:
                    raise ValueError("The model upload ended before the whole file arrived.")
                output.write(chunk)
                remaining -= len(chunk)
        partial.replace(target)
    except Exception:
        partial.unlink(missing_ok=True)
        raise
    return {"ok": True, "path": str(target), "name": filename, "is_dir": False}


class OutputFileExistsError(FileExistsError):
    def __init__(self, path):
        self.path = str(path)
        super().__init__(f"Output file already exists: {self.path}")


def requested_output_file(payload):
    settings = payload.get("settings") or {}
    if settings.get("output_to_mpv"):
        return None
    input_path = str(payload.get("input_path") or "")
    folder = str(payload.get("output_folder") or "")
    if not input_path or not folder:
        return None
    encoding = payload.get("encoding") or {}
    ext = str(encoding.get("video_container") or settings.get("video_container") or "mkv").lstrip(".")
    return Path(folder).expanduser() / (Path(input_path).stem + "_enhanced." + ext)


def build_rve_command(payload):
    settings = payload.get("settings") or {}
    toggles = payload.get("toggles") or {}
    models = payload.get("models") or {}
    encoding = payload.get("encoding") or {}
    input_path = str(payload.get("input_path") or "")
    if not input_path or not Path(input_path).is_file():
        raise ValueError("Select an input video that exists on the backend machine.")
    backend = str(payload.get("backend") or "pytorch").lower()
    if backend == "auto":
        backend = "pytorch"
    python = PYTHON
    if not python or (not Path(python).exists() and not shutil.which(python)):
        raise ValueError("The RVE Python runtime is not installed. Open Runtime and install a rendering backend first.")
    # Bootstrap has already made this expensive first-import probe. Reusing it
    # avoids launching two more Python processes and timing out on CUDA startup.
    capabilities = python_capabilities()
    if not selected_backend_is_available(capabilities, backend):
        detail = capabilities.get("backend_probe_output") or f"The managed runtime did not report {backend} and OpenCV."
        raise ValueError(f"RVE rendering cannot start with {backend}: " + detail)
    cmd = [python, str(BACKEND), "--input", input_path, "--backend", backend, "--cwd", str(APP_DATA)]
    ffmpeg = find_ffmpeg_path()
    if ffmpeg:
        cmd += ["--ffmpeg_path", ffmpeg]
    out_folder = str(payload.get("output_folder") or "")
    if settings.get("output_to_mpv"):
        cmd.append("--output_to_mpv")
    else:
        if not out_folder:
            raise ValueError("Choose an output folder.")
        folder = Path(out_folder).expanduser()
        folder.mkdir(parents=True, exist_ok=True)
        output = requested_output_file(payload)
        if output.exists() and not settings.get("overwrite"):
            raise OutputFileExistsError(output)
        cmd += ["--output", str(output)]
        render["output_file"] = str(output)
    render["output_file"] = render.get("output_file")
    flags = {
        "upscale": ("--upscale_model", "upscale"),
        "interpolate": ("--interpolate_model", "interpolate"),
        "decompress": ("--extra_restoration_models", "decompress"),
        "denoise": ("--extra_restoration_models", "denoise"),
        "deblur": ("--extra_restoration_models", "deblur"),
    }
    resolved = {}
    restoration_model_index = 0
    denoise_model_index = None
    deblur_model_index = None
    for feature, (flag, key) in flags.items():
        if toggles.get(feature) and not models.get(key):
            raise ValueError(f"Choose a {feature} model before starting the render.")
        if toggles.get(feature) and models.get(key):
            path = resolve_model(models[key], backend, key)
            if not path:
                raise ValueError(f"The selected {feature} model is not installed or could not be uniquely matched: {models[key]}")
            resolved[key] = path
            cmd += [flag, path]
            if feature in ("decompress", "denoise", "deblur"):
                if feature == "denoise":
                    denoise_model_index = restoration_model_index
                elif feature == "deblur":
                    deblur_model_index = restoration_model_index
                restoration_model_index += 1
    if denoise_model_index is not None:
        strength = min(10, max(1, int(payload.get("denoise_strength") or 5)))
        temporal = str(payload.get("denoise_temporal") or "off").lower()
        if temporal not in ("off", "light", "medium", "strong"):
            temporal = "off"
        cmd += ["--denoise_model_index", str(denoise_model_index), "--denoise_strength", str(strength),
                "--denoise_temporal", temporal]
        if payload.get("denoise_keep_film_grain"):
            cmd.append("--denoise_keep_film_grain")
    if deblur_model_index is not None:
        strength = min(10, max(1, int(payload.get("deblur_strength") or 6)))
        edge_recovery = min(100, max(0, int(payload.get("deblur_edge_recovery") or 0)))
        ghosting_guard = str(payload.get("deblur_ghosting_guard") or "auto").lower()
        if ghosting_guard == "auto":
            ghosting_guard = "conservative"
        if ghosting_guard not in ("off", "conservative", "aggressive"):
            ghosting_guard = "conservative"
        cmd += ["--deblur_model_index", str(deblur_model_index), "--deblur_strength", str(strength),
                "--deblur_edge_recovery", str(edge_recovery), "--deblur_ghosting_guard", ghosting_guard]
    if toggles.get("interpolate"):
        if not resolved.get("interpolate"):
            raise ValueError("Choose an installed interpolation model.")
        cmd += ["--interpolate_factor", str(float(payload.get("interpolate_factor") or 2))]
    elif resolved.get("interpolate"):
        raise ValueError("Disable or select interpolation consistently.")
    for key, flag in (("start_time", "--start_time"), ("end_time", "--end_time")):
        value = payload.get(key, settings.get("render_start_time" if key == "start_time" else "render_end_time"))
        if value not in (None, ""):
            cmd += [flag, str(value)]
    simple = {
        "tilesize": settings.get("tile_size", 0), "overlap": settings.get("overlap", 0),
        "precision": settings.get("precision", "auto"), "device": settings.get("device", "auto"),
        "pytorch_gpu_id": settings.get("pytorch_gpu_id", 0), "ncnn_gpu_id": settings.get("ncnn_gpu_id", 0),
        "crf": encoding.get("crf", "18"), "video_encoder_preset": encoding.get("video_encoder_preset", "libx264"),
        "audio_encoder_preset": encoding.get("audio_encoder_preset", "copy_audio"), "subtitle_encoder_preset": encoding.get("subtitle_encoder_preset", "copy_subtitle"),
        "audio_bitrate": encoding.get("audio_bitrate", "192k"), "video_pixel_format": encoding.get("video_pixel_format", "yuv420p"),
    }
    for key, value in simple.items():
        if value is not None and value != "":
            cmd += ["--" + key, str(value)]
    for enabled, flag in ((settings.get("overwrite"), "--overwrite"), (settings.get("auto_border_cropping"), "--border_detect"),
                          (settings.get("uhd_mode"), "--UHD_mode"), (settings.get("auto_hdr_mode"), "--hdr_mode"),
                          (settings.get("dynamic_scaled_optical_flow"), "--dynamic_scaled_optical_flow"), (settings.get("ensemble"), "--ensemble"),
                          (settings.get("output_to_mpv"), "--output_to_mpv"), (settings.get("merge_subtitles", True), "--merge_subtitles"),
                          (settings.get("benchmark_mode"), "--benchmark"), (toggles.get("slomo"), "--slomo_mode")):
        if enabled:
            cmd.append(flag)
    if settings.get("motion_blur"):
        cmd += ["--motion_blur", "--motion_blur_shutter_angle", str(settings.get("motion_blur_shutter_angle", 180)),
                "--motion_blur_strength", str(settings.get("motion_blur_strength", "gaussian_sym"))]
        if not settings.get("motion_blur_gamma", True):
            cmd.append("--no-motion_blur_gamma")
    if settings.get("deduplicate"):
        cmd += ["--deduplicate", "--deduplicate_threshold", str(settings.get("deduplicate_threshold", .9))]
    for key, flag in (("custom_encoder", "--custom_encoder"),):
        if encoding.get(key):
            cmd += [flag, str(encoding[key])]
    return cmd


def build_animescripter_command(payload, info):
    settings = payload.get("settings") or {}
    toggles = payload.get("toggles") or {}
    options = payload.get("animescripter") or {}
    input_path = str(payload.get("input_path") or "")
    if not input_path or not Path(input_path).is_file():
        raise ValueError("Select an input video that exists on the backend machine.")
    folder = Path(str(payload.get("output_folder") or "")).expanduser()
    if settings.get("output_to_mpv"):
        raise ValueError("AnimeScripter does not support RVE's MPV output toggle through this adapter.")
    if not str(folder):
        raise ValueError("Choose an output folder.")
    folder.mkdir(parents=True, exist_ok=True)
    ext = str((payload.get("encoding") or {}).get("video_container") or "mkv").lstrip(".")
    output = folder / (Path(input_path).stem + "_enhanced." + ext)
    cmd = [info["executable"], "--input", input_path, "--output", str(output)]
    # RVE model labels do not name AnimeScripter methods. Use documented
    # AnimeScripter defaults and keep their methods replaceable via request.
    methods = options.get("methods") or {}
    if toggles.get("upscale"):
        cmd += ["--upscale_method", str(methods.get("upscale") or "shufflecugan")]
        scale = int(payload.get("override_upscale_scale") or 2)
        if scale in (2, 3, 4): cmd += ["--upscale_factor", str(scale)]
    if toggles.get("interpolate"):
        cmd += ["--interpolate_method", str(methods.get("interpolate") or "rife4.6"), "--interpolate_factor", str(float(payload.get("interpolate_factor") or 2))]
    if toggles.get("decompress") or toggles.get("denoise") or toggles.get("deblur"):
        cmd += ["--restore_method", str(methods.get("restore") or "anime1080fixer")]
    if options.get("smooth_dedup"):
        if not toggles.get("interpolate"):
            raise ValueError("AnimeScripter smooth dedup needs Interpolate enabled.")
        cmd += ["--smooth_dedup", "--smooth_dedup_method", "ssim", "--smooth_dedup_sens", str(options.get("dedup_sensitivity", 35))]
    elif settings.get("deduplicate"):
        cmd += ["--dedup", "--dedup_method", "ssim", "--dedup_sens", str(round((1 - float(settings.get("deduplicate_threshold", .9))) * 100))]
    # TAS documents depth, segmentation and stabilization as standalone jobs;
    # reject mixes that would silently ignore the user's other enabled options.
    standalone = [name for name in ("depth", "segment", "stabilize") if options.get(name)]
    active_pipeline = any(toggles.get(key) for key in ("upscale", "interpolate", "decompress", "denoise", "deblur")) or settings.get("deduplicate") or options.get("smooth_dedup")
    if len(standalone) > 1 or (standalone and active_pipeline):
        raise ValueError("AnimeScripter depth, segment and stabilize run as standalone operations. Enable only one of them per job, without upscaling/interpolation/restoration/dedup.")
    if options.get("depth"): cmd += ["--depth_method", str(methods.get("depth") or "small_v2"), "--depth_quality", str(options.get("depth_quality") or "low")]
    if options.get("segment"): cmd += ["--segment_method", str(methods.get("segment") or "anime")]
    if options.get("stabilize"): cmd += ["--stabilize_method", str(methods.get("stabilize") or "classic")]
    if payload.get("start_time") not in (None, ""): cmd += ["--inpoint", str(payload["start_time"])]
    if payload.get("end_time") not in (None, ""): cmd += ["--outpoint", str(payload["end_time"])]
    render["output_file"] = str(output)
    return animescripter_invocation(cmd[0], cmd[1:])


def prepare_and_start_render(payload, engine, output_file):
    pause_memory = None
    preview_memory = None
    try:
        if render_cancel_event.is_set():
            raise InterruptedError("Render cancelled before engine startup.")
        if engine != "animescripter":
            capabilities = python_capabilities()
            backend = str(payload.get("backend") or "pytorch").lower()
            if backend == "auto":
                backend = "pytorch"
            if not selected_backend_is_available(capabilities, backend):
                probe = str(capabilities.get("backend_probe_output") or f"{backend} and OpenCV were not detected in the managed Python runtime.").strip()
                raise RuntimeError(f"The selected {backend} backend is unavailable. Open Runtime and install or select an available backend. Details: " + probe)
        ensure_render_models(payload)
        if render_cancel_event.is_set():
            raise InterruptedError("Render cancelled before the engine started.")
        with state_lock:
            if render.get("status") != "preparing":
                return
            if engine == "animescripter":
                info = animescripter_status()
                if not info["available"]:
                    raise RuntimeError(info["detail"])
                cmd = build_animescripter_command(payload, info)
            else:
                cmd = build_rve_command(payload)
            preview_enabled = bool((payload.get("settings") or {}).get("preview_enabled", True))
            if engine != "animescripter":
                pause_memory = shared_memory.SharedMemory(create=True, size=1, name="rve_pause_" + uuid.uuid4().hex)
                pause_memory.buf[0] = 0
                cmd += ["--pause_shared_memory_id", pause_memory.name]
                if preview_enabled:
                    # The renderer downsizes live frames into a bounded shared-memory packet.
                    preview_memory = shared_memory.SharedMemory(create=True, size=8 * 1024 * 1024 + 16, name="rve_preview_" + uuid.uuid4().hex)
                    preview_memory.buf[:16] = bytes(16)
                    cmd += ["--preview_shared_memory_id", preview_memory.name]
            APP_DATA.mkdir(parents=True, exist_ok=True)
            proc = subprocess.Popen(cmd, cwd=APP_DATA, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                    text=True, encoding="utf-8", errors="replace", bufsize=1,
                                    **hidden_subprocess_kwargs())
            render.update(status="running", error="", paused=False, process=proc,
                          pause_memory=pause_memory, preview_memory=preview_memory, preview_jpeg=None,
                          preview_size=8 * 1024 * 1024 + 16 if preview_memory else 0,
                          preview_enabled=preview_enabled, engine=engine,
                          log_lines=[f"Starting {engine} renderer...", "Command: " + subprocess.list2cmdline(cmd)],
                          current_frame=0, gen_fps=None, eta="--", output_file=str(output_file) if output_file else None)
        threading.Thread(target=read_render_output, args=(proc, pause_memory, preview_memory), daemon=True).start()
    except Exception as exc:
        for memory in (pause_memory, preview_memory):
            if memory is not None:
                try:
                    memory.close()
                    memory.unlink()
                except (FileNotFoundError, BufferError):
                    pass
        with state_lock:
            render.update(status="stopped" if isinstance(exc, InterruptedError) else "error", error="" if isinstance(exc, InterruptedError) else str(exc), paused=False, process=None,
                          pause_memory=None, preview_memory=None, preview_size=0)
            render["log_lines"] = (render.get("log_lines") or []) + ["Render cancelled." if isinstance(exc, InterruptedError) else str(exc)]


class Handler(BaseHTTPRequestHandler):
    server_version = "RVEWebBridge/1.0"

    def log_message(self, fmt, *args):
        stream = getattr(sys, "stderr", None)
        if stream is not None:
            stream.write("[rve-api] " + fmt % args + "\n")

    def read_body(self):
        try:
            length = min(int(self.headers.get("Content-Length", "0")), 2_000_000)
            return json.loads(self.rfile.read(length) or b"{}")
        except (ValueError, UnicodeDecodeError):
            raise ValueError("Request body must be valid JSON.")

    def do_OPTIONS(self):
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", "http://127.0.0.1")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.end_headers()

    def do_GET(self):
        url = urllib.parse.urlsplit(self.path)
        path = url.path
        if path == "/openapi.json":
            respond_json(self, {"openapi": "3.0.0", "info": {"title": "RVE local bridge", "version": "1.0.0"}, "servers": [{"url": "http://127.0.0.1:5173"}], "paths": {"/api/bootstrap": {}, "/api/models": {}, "/api/render/start": {}, "/api/render/status": {}}}); return
        if path == "/api/bootstrap":
            system = python_capabilities(); disk = shutil.disk_usage(APP_DATA)
            system["available_disk_gb"] = round(disk.free / (1024 ** 3), 2)
            backend, catalog = model_catalog("pytorch")
            models = {category: list(entries) for category, entries in catalog.items()}
            models.update({"backend": backend, "model_status": catalog_model_status(catalog)})
            respond_json(self, {"system": system, "models": models, "settings": load_settings(),
                                "startup": {"install_state": install_state, "needs_runtime_install": False,
                                            "animescripter_available": bool(find_animescripter())}}); return
        if path == "/api/startup/install_status":
            with install_lock: state = dict(install_state)
            respond_json(self, state); return
        if path == "/api/storage/status":
            respond_json(self, storage_snapshot()); return
        if path == "/api/app/update/check":
            try:
                release = check_github_update()
                with update_lock:
                    update_state.clear()
                    update_state.update(release, progress=0, release=release)
                respond_json(self, release)
            except Exception as exc:
                with update_lock:
                    update_state.update(status="error", error=str(exc), detail=f"Could not check GitHub for updates: {exc}")
                respond_json(self, {**update_state, "error": str(exc)}, 503)
            return
        if path == "/api/app/update/status":
            with update_lock:
                respond_json(self, dict(update_state))
            return
        if path == "/api/python/info":
            try: detail = subprocess.run([PYTHON, "--version"], capture_output=True, text=True, timeout=4, **hidden_subprocess_kwargs()).stdout.strip() or subprocess.run([PYTHON, "--version"], capture_output=True, text=True, timeout=4, **hidden_subprocess_kwargs()).stderr.strip()
            except Exception as exc: detail = str(exc)
            respond_json(self, {"python_path": PYTHON or None, "python_available": bool(PYTHON and (Path(PYTHON).exists() or shutil.which(PYTHON))), "python_detail": detail, "pip_detail": "Use the runtime's Python environment.", "version": detail}); return
        if path == "/api/models":
            backend, catalog = model_catalog(urllib.parse.parse_qs(url.query).get("backend", ["pytorch"])[0])
            models = {category: list(entries) for category, entries in catalog.items()}
            models.update({"backend": backend, "model_status": catalog_model_status(catalog)})
            respond_json(self, models); return
        if path == "/api/models/download_status":
            with download_lock:
                snapshot = dict(download_job)
            respond_json(self, snapshot); return
        if path == "/api/custom_models": respond_json(self, custom_models_payload()); return
        if path == "/api/runtime_metrics": respond_json(self, {"gpu_load": None, "throughput": None, "pipeline_health": status_snapshot()["status"], "available": False}); return
        if path == "/api/render/status": respond_json(self, status_snapshot()); return
        if path == "/api/render/preview_frame":
            with state_lock:
                memory = render.get("preview_memory")
                available = bool(render.get("preview_enabled") and render.get("engine") == "rve")
                cached_frame = render.get("preview_jpeg")
            if not available:
                self.send_response(204); self.send_header("Cache-Control", "no-store"); self.end_headers(); return
            content = read_preview_jpeg(memory) or cached_frame
            if not content:
                self.send_response(204); self.send_header("Cache-Control", "no-store"); self.end_headers(); return
            self.send_response(200)
            self.send_header("Content-Type", "image/jpeg")
            self.send_header("Content-Length", str(len(content)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(content)
            return
        if path == "/api/video/first_frame":
            video = urllib.parse.parse_qs(url.query).get("path", [""])[0]
            try:
                frame = inspect_video(video, include_frame=True).get("frame")
                if not frame:
                    respond_json(self, {"error": "The selected video did not yield a preview frame."}, 422); return
                content = __import__("base64").b64decode(frame)
                self.send_response(200)
                self.send_header("Content-Type", "image/jpeg")
                self.send_header("Content-Length", str(len(content)))
                self.send_header("Cache-Control", "no-store")
                self.end_headers()
                self.wfile.write(content)
            except Exception as exc:
                respond_json(self, {"error": f"Video preview failed: {exc}"}, 503)
            return
        if path == "/api/animescripter/status": respond_json(self, animescripter_status()); return
        respond_json(self, {"error": "This endpoint is for the native desktop app."}, 404)

    def do_POST(self):
        try:
            parsed_url = urllib.parse.urlsplit(self.path)
            path = parsed_url.path
            if path == "/api/custom_models/upload":
                query = urllib.parse.parse_qs(parsed_url.query)
                result = receive_custom_model_upload(self, query.get("filename", [""])[0])
                respond_json(self, result, 201)
                return
            payload = self.read_body()
            if path == "/api/settings":
                current = load_settings(); current.update(payload.get("settings") or {}); save_settings(current); respond_json(self, {"ok": True, "settings": current}); return
            if path == "/api/settings/reset":
                save_settings({}); respond_json(self, {"ok": True, "settings": {}}); return
            if path == "/api/storage/configure":
                mode = str(payload.get("storage") or "").lower()
                if mode not in ("wsl", "windows"):
                    respond_json(self, {"error": "Choose WSL or Windows model storage."}, 400); return
                current = load_settings()
                current["model_storage"] = mode
                save_settings(current)
                os.environ["RVE_MODEL_STORAGE"] = mode
                respond_json(self, begin_model_storage_setup(mode), 202); return
            if path == "/api/app/update/download":
                respond_json(self, begin_update_download(), 202)
                return
            if path == "/api/models/status":
                respond_json(self, {"model_status": model_status(payload.get("models") or {}, payload.get("backend", "pytorch"))}); return
            if path in ("/api/models/download", "/api/actions/download_models"):
                backend = payload.get("backend", "pytorch")
                if "selections" in payload:
                    selections = payload.get("selections") or {}
                    categories = None
                else:
                    selections = None
                    categories = payload.get("categories") or []
                result = start_model_download(backend, selections=selections, categories=categories)
                respond_json(self, result, 202); return
            if path == "/api/startup/install":
                result = begin_runtime_operation("recommended", "pytorch", payload)
                respond_json(self, result, 202); return
            if path == "/api/actions/cancel":
                cancel_event.set()
                render_cancel_event.set()
                update_cancel_event.set()
                with update_process_lock:
                    update_proc = update_process
                if update_proc and update_proc.poll() is None:
                    try:
                        update_proc.terminate()
                    except OSError:
                        pass
                with runtime_process_lock:
                    proc = runtime_process
                if proc and proc.poll() is None:
                    try:
                        proc.terminate()
                    except OSError:
                        pass
                with storage_process_lock:
                    storage_proc = storage_process
                if storage_proc and storage_proc.poll() is None:
                    try:
                        storage_proc.terminate()
                    except OSError:
                        pass
                render_proc = render.get("process")
                if render_proc and render_proc.poll() is None:
                    try:
                        render_proc.terminate()
                        render.update(status="stopping", paused=False)
                    except OSError:
                        pass
                respond_json(self, {"ok": True, "detail": "Cancellation requested for active work."}); return
            if path == "/api/data/delete":
                respond_json(self, delete_managed_data(str(payload.get("kind") or ""))); return
            if path == "/api/animescripter/install":
                result = begin_runtime_operation("animescripter", payload=payload)
                respond_json(self, result, 202); return
            if path == "/api/python/pip":
                if not PYTHON or not Path(PYTHON).is_file():
                    respond_json(self, {"ok": False, "error": "Managed Python is not installed. Run Recommended Runtime Setup first."}, 409); return
                command = str(payload.get("command") or "--version").strip()
                allowed = {"--version", "list", "freeze"}
                if command not in allowed:
                    respond_json(self, {"ok": False, "error": "Only uv --version, uv pip list and uv pip freeze are available from this screen."}, 400); return
                uv = managed_uv_path()
                if not uv.is_file():
                    respond_json(self, {"ok": False, "error": "The app-managed uv tool is not installed yet. Run Runtime setup first."}, 409); return
                uv_command = [str(uv), "--version"] if command == "--version" else [str(uv), "pip", command, "--python", PYTHON]
                proc = subprocess.run(uv_command,
                                      capture_output=True, text=True, timeout=min(int(payload.get("timeout", 30)), 120),
                                      creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
                respond_json(self, {"ok": proc.returncode == 0, "command": subprocess.list2cmdline(uv_command),
                                    "output": (proc.stdout + proc.stderr)[-20000:], "returncode": proc.returncode}); return
            if path == "/api/video/inspect":
                selected = str(payload.get("path") or "").strip()
                if not selected:
                    respond_json(self, {"error": "Choose a video file first."}, 400); return
                info = inspect_video(selected)
                respond_json(self, {**info, "path": str(Path(selected).expanduser().resolve())}); return
            if path == "/api/dialog/select_input":
                selected = choose_path("input", load_settings().get("advanced", {}).get("last_input_folder_location"))
                info = {}
                if selected:
                    try:
                        info = inspect_video(selected)
                    except Exception as exc:
                        # Keep the user's file selection even when optional
                        # metadata probing is unavailable. The render endpoint
                        # performs its own validation and returns the actionable
                        # error there; probing must not make the picker unusable.
                        info = {"error": f"Video metadata is unavailable: {exc}"}
                    settings = load_settings()
                    advanced = settings.get("advanced", {})
                    advanced["last_input_folder_location"] = str(Path(selected).parent)
                    settings["advanced"] = advanced
                    save_settings(settings)
                respond_json(self, {"path": selected or "", "video_info": info}); return
            if path == "/api/dialog/select_inputs":
                folder = choose_path("batch_folder", load_settings().get("advanced", {}).get("last_input_folder_location"))
                candidates = list_videos_in_folder(folder) if folder else []
                paths, info, invalid = inspect_videos(candidates) if candidates else ([], {}, [])
                if folder:
                    settings = load_settings()
                    advanced = settings.get("advanced", {})
                    advanced["last_input_folder_location"] = folder
                    settings["advanced"] = advanced
                    save_settings(settings)
                respond_json(self, {"path": folder or "", "paths": paths, "video_info": info,
                                    "detail": (f"Loaded {len(paths)} valid video(s); skipped {len(invalid)} unreadable file(s)."
                                               if paths and invalid else f"Loaded {len(paths)} valid video(s) in the selected folder."
                                               if paths else "No valid videos were found in the selected folder.")}); return
            if path == "/api/dialog/select_output":
                selected = choose_path("folder", load_settings().get("advanced", {}).get("output_folder_location"))
                respond_json(self, {"path": selected or ""}); return
            if path == "/api/dialog/select_custom_model":
                selected = choose_path("model", str(custom_models_dir()))
                respond_json(self, {"path": selected or ""}); return
            if path in ("/api/actions/open_rve_folder", "/api/actions/open_custom_models_folder", "/api/dialog/open_output"):
                target = Path(payload.get("path") or (custom_models_dir() if path.endswith("open_custom_models_folder") else APP_DATA)).expanduser().resolve()
                if path.endswith("open_output") and not target.exists():
                    respond_json(self, {"ok": False, "error": f"Folder does not exist: {target}"}, 404); return
                target.mkdir(parents=True, exist_ok=True) if path.endswith("open_custom_models_folder") else None
                if os.name == "nt": os.startfile(str(target))
                else: raise ValueError("Opening desktop folders is only supported on Windows.")
                respond_json(self, {"ok": True, "path": str(target)}); return
            if path in ("/api/actions/import_custom_models", "/api/actions/import_custom_model_folder"):
                if path.endswith("folder"):
                    folder = choose_path("folder", str(Path.home()))
                    suffixes = {".pth", ".pt", ".onnx", ".safetensors", ".engine", ".bin", ".param", ".ckpt"}
                    sources = [str(item) for item in Path(folder).rglob("*") if item.is_file() and item.suffix.lower() in suffixes] if folder else []
                else:
                    sources = choose_path("models", str(Path.home()))
                imported, failures = import_custom_models(sources)
                respond_json(self, custom_models_payload() | {"imported": imported, "failures": failures}); return
            if path == "/api/actions/delete_custom_model":
                model_root = custom_models_dir().resolve()
                target = (model_root / str(payload.get("name") or "")).resolve()
                if target.parent != model_root or not target.exists():
                    respond_json(self, {"error": "Custom model was not found."}, 404); return
                if target.is_dir(): shutil.rmtree(target)
                else: target.unlink()
                respond_json(self, custom_models_payload()); return
            if path == "/api/render/start":
                engine = str(payload.get("engine") or "rve").lower()
                output_file = requested_output_file(payload)
                if output_file and output_file.exists() and not (payload.get("settings") or {}).get("overwrite"):
                    raise OutputFileExistsError(output_file)
                with state_lock:
                    if render.get("status") in ("preparing", "running", "paused") or (
                        render.get("process") is not None and render["process"].poll() is None
                    ):
                        respond_json(self, {"ok": False, "error": "A render is already running."}, 409); return
                    render_cancel_event.clear()
                    cancel_event.clear()
                    render.update(status="preparing", error="", paused=False, process=None,
                                  pause_memory=None, preview_memory=None, preview_jpeg=None,
                                  preview_size=0, current_frame=0, gen_fps=None, eta="--",
                                  started=time.time(), output_file=str(output_file) if output_file else None,
                                  log_lines=["Preparing renderer and selected models…"])
                threading.Thread(target=prepare_and_start_render, args=(payload, engine, output_file),
                                 name="rve-render-prepare", daemon=True).start()
                respond_json(self, {"ok": True, "status": "preparing"}, 202); return
            if path in ("/api/render/pause", "/api/render/stop"):
                proc = render.get("process")
                if path.endswith("stop") and render.get("status") == "preparing":
                    render_cancel_event.set()
                    cancel_event.set()
                    respond_json(self, {"ok": True, "status": "preparing"}); return
                if not proc or proc.poll() is not None:
                    respond_json(self, {"ok": True, "paused": False, "status": render["status"]}); return
                if path.endswith("pause"):
                    memory = render.get("pause_memory")
                    if memory is None:
                        respond_json(self, {"ok": False, "error": "Pause/resume is unavailable for this processing engine."}, 501); return
                    paused = bool(payload.get("paused"))
                    memory.buf[0] = 1 if paused else 0
                    render.update(paused=paused, status="paused" if paused else "running")
                    respond_json(self, {"ok": True, "paused": paused, "status": render["status"]}); return
                proc.terminate(); render.update(status="stopping", paused=False); respond_json(self, {"ok": True, "status": "stopping"}); return
            if path == "/api/actions/verify_backend":
                sysinfo = python_capabilities(refresh=True); ok = bool(sysinfo.get("available_backends")); respond_json(self, {"ok": ok, "backend": payload.get("backend", "auto"), "checks": [{"name": name, "ok": name in sysinfo["available_backends"], "detail": "Available in the managed Python runtime." if name in sysinfo["available_backends"] else "Not installed or unavailable."} for name in ("pytorch", "ncnn", "tensorrt")], "output": "Detected: " + ", ".join(sysinfo["available_backends"]) if ok else "No rendering backend is importable in the managed runtime."}); return
            if path == "/api/actions/refresh_backends":
                sysinfo = python_capabilities(refresh=True)
                respond_json(self, sysinfo); return
            if path == "/api/actions/manage_backend":
                action = str(payload.get("action") or "").lower()
                backend = str(payload.get("backend") or "").lower()
                if action not in ("install", "uninstall"):
                    raise ValueError("Choose install or uninstall.")
                result = begin_runtime_operation(action, backend, payload)
                respond_json(self, {"ok": True, "state": result}, 202); return
            if path == "/api/app/quit":
                # Stop active rendering as part of app exit so the renderer
                # cannot keep the launcher alive or continue writing output.
                proc = render.get("process")
                if proc and proc.poll() is None:
                    try:
                        proc.terminate()
                        render.update(status="stopped", paused=False)
                    except OSError:
                        pass
                respond_json(self, {"ok": True, "status": "shutting_down"})
                # BaseServer.shutdown() must run off the serve_forever thread.
                # Let the local API response return before shutting down.
                threading.Timer(0.15, self.server.shutdown).start()
                return
            if path.startswith("/api/actions/"):
                respond_json(self, {"ok": True, "detail": "No additional action is required."}); return
            respond_json(self, {"error": "Not found"}, 404)
        except ValueError as exc:
            respond_json(self, {"ok": False, "error": str(exc)}, 400)
        except OutputFileExistsError as exc:
            respond_json(self, {"ok": False, "code": "OUTPUT_EXISTS", "output_file": exc.path, "error": str(exc)}, 409)
        except FileExistsError as exc:
            respond_json(self, {"ok": False, "error": str(exc)}, 409)
        except OSError as exc:
            respond_json(self, {"ok": False, "error": f"Model import failed: {exc}"}, 500)
        except RuntimeError as exc:
            respond_json(self, {"ok": False, "error": str(exc)}, 500)
        except Exception as exc:
            respond_json(self, {"ok": False, "error": str(exc)}, 500)


def read_render_output(proc, memory=None, preview_memory=None):
    try:
        for line in proc.stdout:
            line = line.strip()
            if not line: continue
            with state_lock:
                if render.get("process") is not proc:
                    continue
                render["log_lines"].append(line)
                render["log_lines"] = render["log_lines"][-500:]
                import re
                fps = re.search(r"FPS:\s*([\d.]+)", line)
                frame = re.search(r"Current Frame:\s*(\d+)", line)
                eta = re.search(r"ETA:\s*(.+)$", line)
                if fps: render["gen_fps"] = fps.group(1)
                if frame: render["current_frame"] = int(frame.group(1))
                if eta: render["eta"] = eta.group(1).strip()
        code = proc.wait()
        final_preview = read_preview_jpeg(preview_memory)
        with state_lock:
            if render.get("process") is proc:
                if final_preview:
                    render["preview_jpeg"] = final_preview
                if render["status"] not in ("stopped", "error"):
                    render["status"] = "completed" if code == 0 else "error"
                if code:
                    details = [line for line in render.get("log_lines", []) if line.strip()]
                    render["error"] = "\n".join(details[-8:])[-1600:] or f"Renderer exited with code {code}. Check the Logs tab for details."
                render["paused"] = False
                render["pause_memory"] = None
                render["preview_memory"] = None
                render["preview_size"] = 0
        if memory is not None:
            try:
                memory.close(); memory.unlink()
            except FileNotFoundError:
                pass
        if preview_memory is not None:
            try:
                preview_memory.close(); preview_memory.unlink()
            except FileNotFoundError:
                pass
    except Exception as exc:
        if preview_memory is not None:
            try:
                preview_memory.close(); preview_memory.unlink()
            except (FileNotFoundError, BufferError):
                pass
        with state_lock:
            if render.get("process") is proc:
                render["preview_memory"] = None
                render["pause_memory"] = None
                render["preview_size"] = 0
                render["status"] = "error"
                render["log_lines"].append(str(exc))


class SingleOwnerHTTPServer(ThreadingHTTPServer):
    # Windows otherwise permits multiple local Python servers to share the
    # same address with SO_REUSEADDR, splitting render state across processes.
    allow_reuse_address = False
    daemon_threads = True


def run_server():
    APP_DATA.mkdir(parents=True, exist_ok=True)
    try:
        server = SingleOwnerHTTPServer(("127.0.0.1", PORT), Handler)
    except OSError as exc:
        # Packaged Windows services run without a console. Letting this escape
        # PyInstaller's entry point displays an unhandled-exception dialog when
        # another app instance already owns the fixed loopback port.
        print(f"RVE local engine could not bind 127.0.0.1:{PORT}: {exc}", flush=True)
        return

    if os.name == "nt" and os.environ.get("RVE_AUTO_SETUP_MODELS", "1") != "0":
        initial_mode = os.environ.get("RVE_MODEL_STORAGE", "wsl").lower()
        if initial_mode in ("wsl", "windows"):
            begin_model_storage_setup(initial_mode)
    managed_runtime_ready = any(path.is_file() for path in runtime_candidates)
    if (os.name == "nt" and FROZEN and os.environ.get("RVE_AUTO_SETUP_RUNTIME", "1") != "0"
            and not managed_runtime_ready):
        begin_runtime_operation("recommended", "pytorch")
    address, port = server.server_address[:2]
    print(f"RVE local engine service listening on http://{address}:{port}/", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    run_server()
