"""Launch the native Rust desktop app from a source checkout."""

from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parent
APP = ROOT / "release-stage" / "REAL-Video-Enhancer" / "REAL-Video-Enhancer.exe"
if not APP.is_file():
    APP = ROOT / "native-ui" / "target" / "release" / "rve-native.exe"

if APP.is_file():
    raise SystemExit(subprocess.call([str(APP)], cwd=APP.parent))

raise SystemExit(subprocess.call(
    ["cargo", "run", "--release", "--manifest-path", str(ROOT / "native-ui" / "Cargo.toml")],
    cwd=ROOT,
))
