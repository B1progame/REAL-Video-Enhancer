"""Runtime slimming and per-model management in the local engine service."""

import pytest

from backend import web_api


def make_runtime(tmp_path):
    torch = tmp_path / "python" / "Lib" / "site-packages" / "torch"
    (torch / "lib").mkdir(parents=True)
    (torch / "include").mkdir()
    (torch / "lib" / "dnnl.lib").write_bytes(b"x" * 100)
    (torch / "lib" / "torch_cpu.dll").write_bytes(b"x" * 50)
    (torch / "include" / "header.h").write_bytes(b"x" * 10)
    (torch / "__init__.py").write_text("")
    return tmp_path, torch


def test_slim_removes_only_build_files(tmp_path):
    root, torch = make_runtime(tmp_path)
    assert web_api.reclaimable_runtime_bytes(root) == 110

    freed, failures = web_api.slim_runtime(root)

    assert (freed, failures) == (110, [])
    assert not (torch / "lib" / "dnnl.lib").exists()
    assert not (torch / "include").exists()
    assert (torch / "lib" / "torch_cpu.dll").exists()
    assert (torch / "__init__.py").exists()


def test_slim_ignores_a_missing_runtime(tmp_path):
    assert web_api.slim_runtime(tmp_path / "nope") == (0, [])


def test_delete_catalog_model_removes_only_that_file(tmp_path, monkeypatch):
    (tmp_path / "a.pth").write_bytes(b"a")
    (tmp_path / "b.pth").write_bytes(b"b")
    catalog = {"upscale": {"A": ["a.pth", "a.pth", 2, "span"]}}
    monkeypatch.setattr(web_api, "activate_wsl_model_dir", lambda: None)
    monkeypatch.setattr(web_api, "model_catalog", lambda backend: ("pytorch", catalog))
    monkeypatch.setattr(web_api, "find_model_file",
                        lambda name: tmp_path / name if (tmp_path / name).exists() else None)

    assert web_api.delete_catalog_model("pytorch", "upscale", "A")["ok"]
    assert not (tmp_path / "a.pth").exists()
    assert (tmp_path / "b.pth").exists()


def test_delete_catalog_model_rejects_unknown_and_escaping_names(monkeypatch):
    catalog = {"upscale": {"Bad": ["../x.pth", "x.pth", 2, "span"]}}
    monkeypatch.setattr(web_api, "activate_wsl_model_dir", lambda: None)
    monkeypatch.setattr(web_api, "model_catalog", lambda backend: ("pytorch", catalog))
    with pytest.raises(ValueError):
        web_api.delete_catalog_model("pytorch", "upscale", "Missing")
    with pytest.raises(ValueError):
        web_api.delete_catalog_model("pytorch", "upscale", "Bad")
