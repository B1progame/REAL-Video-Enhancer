#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CATALOG="$ROOT/backend/model-catalog.json"
MODEL_DIR="${RVE_MODEL_DIR:-$HOME/.local/share/REAL-Video-Enhancer/models}"
WORKERS="${RVE_MODEL_WORKERS:-4}"
BASE_URL="https://github.com/TNTwise/real-video-enhancer-models/releases/download/models"

if ! command -v python3 >/dev/null 2>&1; then
  echo "Python 3 is required. Install it with: sudo apt install python3" >&2
  exit 1
fi
if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required. Install it with: sudo apt install curl" >&2
  exit 1
fi
if [[ ! -f "$CATALOG" ]]; then
  echo "Model catalog not found: $CATALOG" >&2
  exit 1
fi

mkdir -p "$MODEL_DIR" "$MODEL_DIR/.downloads"
export RVE_CATALOG="$CATALOG" RVE_MODEL_DIR="$MODEL_DIR" RVE_BASE_URL="$BASE_URL" RVE_MODEL_WORKERS="$WORKERS"

python3 - <<'PY'
import concurrent.futures
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import urllib.parse

catalog_path = pathlib.Path(os.environ["RVE_CATALOG"])
model_dir = pathlib.Path(os.environ["RVE_MODEL_DIR"]).expanduser()
download_dir = model_dir / ".downloads"
base_url = os.environ["RVE_BASE_URL"].rstrip("/")
workers = max(1, min(8, int(os.environ.get("RVE_MODEL_WORKERS", "4"))))

with catalog_path.open(encoding="utf-8") as stream:
    catalog = json.load(stream)

specs = set()
for backend in catalog.values():
    for category in backend.values():
        for spec in category.values():
            if isinstance(spec, list) and len(spec) > 1:
                target, source = str(spec[0]), str(spec[1])
                if (pathlib.PurePosixPath(target).name != target
                        or pathlib.PurePosixPath(source).name != source
                        or not re.fullmatch(r"[A-Za-z0-9_.+-]+", target)
                        or not re.fullmatch(r"[A-Za-z0-9_.+-]+", source)):
                    raise SystemExit(f"Unsafe model name in catalog: {target!r}, {source!r}")
                specs.add((target, source))

ordered = sorted(specs)
sources = sorted({source for _, source in ordered})
print(f"Preparing {len(ordered)} model entries from {len(sources)} unique downloads.")
print(f"Destination: {model_dir}")
print("All backends are included; the complete set may require substantial disk space.")

def download(source):
    required_targets = [target for target, item_source in ordered if item_source == source]
    if all((model_dir / target).exists()
           or (source.endswith(".tar.gz") and (model_dir / source.removesuffix(".tar.gz")).is_dir())
           for target in required_targets):
        return source, "already installed"
    if source.endswith(".tar.gz"):
        archive = download_dir / source
        expected_archive = archive
        output_target = archive
    else:
        output_target = model_dir / source
        expected_archive = output_target
    if expected_archive.exists() and expected_archive.stat().st_size > 0:
        return source, "already downloaded"
    url = f"{base_url}/{urllib.parse.quote(source)}"
    partial = output_target.with_name(output_target.name + ".part")
    command = ["curl", "--fail", "--location", "--silent", "--show-error", "--retry", "3", "--continue-at", "-", "--output", str(partial), url]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        partial.unlink(missing_ok=True)
        raise RuntimeError(f"{source}: {result.stderr.strip() or 'download failed'}")
    if not partial.exists() or partial.stat().st_size == 0:
        partial.unlink(missing_ok=True)
        raise RuntimeError(f"{source}: the download was empty")
    partial.replace(output_target)
    return source, f"downloaded ({output_target.stat().st_size / (1024 * 1024):.1f} MiB)"

failures = []
with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
    futures = [pool.submit(download, source) for source in sources]
    for index, future in enumerate(concurrent.futures.as_completed(futures), start=1):
        try:
            name, detail = future.result()
            print(f"[{index}/{len(futures)}] {name}: {detail}", flush=True)
        except Exception as error:
            failures.append(str(error))
            print(f"[{index}/{len(futures)}] ERROR: {error}", flush=True)

for archive_path in sorted(download_dir.glob("*.tar.gz")):
    try:
        with tarfile.open(archive_path, "r:gz") as archive:
            members = archive.getmembers()
            for member in members:
                member_path = pathlib.PurePosixPath(member.name)
                if member_path.is_absolute() or ".." in member_path.parts or member.issym() or member.islnk() or member.isdev():
                    raise RuntimeError(f"unsafe archive member: {member.name}")
            archive.extractall(model_dir, members=members)
        archive_path.unlink()
        print(f"Extracted {archive_path.name}", flush=True)
    except Exception as error:
        failures.append(f"{archive_path.name}: {error}")
        print(f"ERROR extracting {archive_path.name}: {error}", flush=True)

# Some catalog folder names are aliases for the folder actually packed in the
# upstream archive (for example up3x-conservative -> up2x-conservative).
for target, source in ordered:
    if target != source and not (model_dir / target).exists() and source.endswith(".tar.gz"):
        packed_folder = model_dir / source.removesuffix(".tar.gz")
        if packed_folder.is_dir():
            shutil.copytree(packed_folder, model_dir / target)
            print(f"Mapped {target} to archive folder {packed_folder.name}", flush=True)

for folder in [download_dir]:
    if folder.exists() and not any(folder.iterdir()):
        folder.rmdir()

missing = [target for target, _ in ordered if not (model_dir / target).exists()]
if failures or missing:
    if missing:
        print(f"{len(missing)} catalog entries are still missing after extraction.", file=sys.stderr)
    if failures:
        print(f"{len(failures)} downloads or extractions failed.", file=sys.stderr)
    sys.exit(1)

print(f"Installed and verified all {len(ordered)} catalog entries in {model_dir}.")
PY
