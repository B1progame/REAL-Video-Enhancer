"""The slow runtime probe is reused until the installed packages change."""

import os
import subprocess

from backend import web_api


def runtime(tmp_path):
    scripts = tmp_path / "python" / "Scripts"
    scripts.mkdir(parents=True)
    interpreter = scripts / "python.exe"
    interpreter.write_bytes(b"")
    packages = tmp_path / "python" / "Lib" / "site-packages"
    (packages / "torch-2.8.0.dist-info").mkdir(parents=True)
    return interpreter, packages


def fake_probe(calls, returncode=0, stdout="opencv=True\ninstalled_pytorch=True\n"):
    def run(command, **kwargs):
        calls.append(command)
        return subprocess.CompletedProcess(command, returncode, stdout, "")
    return run


def test_probe_is_cached_until_packages_change(tmp_path, monkeypatch):
    interpreter, packages = runtime(tmp_path)
    calls = []
    monkeypatch.setattr(web_api, "APP_DATA", tmp_path)
    monkeypatch.setattr(web_api, "run_hidden_process", fake_probe(calls))

    first = web_api.run_capability_probe(str(interpreter), "code")
    second = web_api.run_capability_probe(str(interpreter), "code")
    assert len(calls) == 1
    assert second.stdout == first.stdout

    (packages / "ncnn-1.0.dist-info").mkdir()
    web_api.run_capability_probe(str(interpreter), "code")
    assert len(calls) == 2

    web_api.run_capability_probe(str(interpreter), "code", refresh=True)
    assert len(calls) == 3


def test_failed_or_empty_probe_is_not_remembered(tmp_path, monkeypatch):
    interpreter, _ = runtime(tmp_path)
    calls = []
    monkeypatch.setattr(web_api, "APP_DATA", tmp_path)
    monkeypatch.setattr(web_api, "run_hidden_process", fake_probe(calls, returncode=1))

    web_api.run_capability_probe(str(interpreter), "code")
    web_api.run_capability_probe(str(interpreter), "code")
    assert len(calls) == 2


def test_changed_probe_code_invalidates_the_cache(tmp_path, monkeypatch):
    interpreter, _ = runtime(tmp_path)
    calls = []
    monkeypatch.setattr(web_api, "APP_DATA", tmp_path)
    monkeypatch.setattr(web_api, "run_hidden_process", fake_probe(calls))

    web_api.run_capability_probe(str(interpreter), "one")
    web_api.run_capability_probe(str(interpreter), "two")
    assert len(calls) == 2
