"""The installer must not start the app as its own child process.

Setup runs with Windows RedirectionGuard, which child processes inherit. An app
started that way cannot follow the junctions uv creates, so runtime setup fails
with "os error 448". Handing the launch to the running Explorer shell avoids it.
"""

from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[3] / "REAL-Video-Enhancer.iss"


def run_entries():
    section, entries = "", []
    for line in SCRIPT.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line.startswith("["):
            section = line
        elif section == "[Run]" and line.startswith("Filename:"):
            entries.append(line)
    return entries


def test_app_is_launched_through_explorer():
    entries = run_entries()
    assert len(entries) == 1
    assert entries[0].startswith('Filename: "{win}\\explorer.exe";')
    assert 'Parameters: """{app}\\{#MyAppExeName}""";' in entries[0]
    assert "nowait" in entries[0]
