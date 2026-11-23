"""Focused tests for selecting a safe in-app Windows update."""

import json
import sys
from subprocess import CompletedProcess
from unittest.mock import patch
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from backend import web_api


def _release(digest="sha256:" + "a" * 64):
    return {
        "tag_name": "v2.4.30",
        "html_url": "https://github.com/B1progame/REAL-Video-Enhancer/releases/tag/v2.4.30",
        "assets": [
            {
                "name": "REAL-Video-Enhancer-2.4.30-Windows-Installer.exe",
                "browser_download_url": (
                    "https://github.com/B1progame/REAL-Video-Enhancer/"
                    "releases/download/v2.4.30/REAL-Video-Enhancer-2.4.30-Windows-Installer.exe"
                ),
                "size": 1234,
                "digest": digest,
            },
            {
                "name": "notes.txt",
                "browser_download_url": "https://example.com/notes.txt",
                "size": 20,
                "digest": "sha256:" + "b" * 64,
            },
        ],
    }


class UpdateFlowTests(unittest.TestCase):
    def test_frozen_build_does_not_fall_back_to_python_on_path(self):
        with patch.object(web_api.shutil, "which", return_value="python.exe") as lookup:
            self.assertIsNone(web_api.resolve_default_python([], frozen=True))

        lookup.assert_not_called()

    def test_source_build_can_fall_back_to_system_python(self):
        with patch.object(web_api.shutil, "which", side_effect=["python.exe", None]):
            self.assertEqual(web_api.resolve_default_python([], frozen=False), "python.exe")

    def test_app_managed_python_is_recognized_by_resolved_path(self):
        with patch.object(web_api, "managed_python_paths", {Path(sys.executable).resolve()}):
            self.assertTrue(web_api.is_managed_python(sys.executable))

    def test_system_python_is_not_misreported_as_app_managed(self):
        with patch.object(web_api, "managed_python_paths", set()):
            self.assertFalse(web_api.is_managed_python(sys.executable))

    def test_missing_python_does_not_launch_an_empty_command(self):
        with patch.object(web_api, "PYTHON", ""), patch.object(web_api.subprocess, "run") as run:
            result = web_api.python_info_payload()

        self.assertFalse(result["python_available"])
        self.assertFalse(result["python_managed"])
        self.assertIn("not installed", result["python_detail"])
        run.assert_not_called()

    def test_system_python_is_reported_as_external_not_managed(self):
        with (
            patch.object(web_api, "PYTHON", sys.executable),
            patch.object(web_api, "managed_python_paths", set()),
            patch.object(web_api.subprocess, "run", return_value=CompletedProcess([], 0, "Python 3.12.0", "")),
        ):
            result = web_api.python_info_payload()

        self.assertTrue(result["python_available"])
        self.assertFalse(result["python_managed"])
        self.assertIn("External Python", result["python_detail"])

    def test_python_probe_oserror_is_reported_as_unavailable(self):
        with (
            patch.object(web_api, "PYTHON", sys.executable),
            patch.object(web_api, "managed_python_paths", {Path(sys.executable).resolve()}),
            patch.object(web_api.subprocess, "run", side_effect=OSError(87, "The parameter is incorrect")),
        ):
            result = web_api.python_info_payload()

        self.assertFalse(result["python_available"])
        self.assertFalse(result["python_managed"])
        self.assertIn("Install the app-managed runtime", result["python_detail"])

    def test_managed_python_environment_path_is_the_venv_root(self):
        with patch.object(web_api, "LOCAL_DATA", Path("/app-data")):
            self.assertEqual(web_api.managed_python_environment_dir(), Path("/app-data/python/python"))
            self.assertEqual(
                web_api.managed_python_path(),
                web_api.managed_python_environment_dir() / "Scripts" / "python.exe"
                if web_api.os.name == "nt"
                else web_api.managed_python_environment_dir() / "bin" / "python3",
            )
            command = web_api.managed_python_venv_command(Path("/app-data/bin/uv.exe"))
            self.assertEqual(command[1:3], ["venv", "--clear"])
            self.assertEqual(command[-1], str(web_api.managed_python_environment_dir()))

    def test_uv_python_data_uses_local_appdata_paths(self):
        with patch.object(web_api, "LOCAL_DATA", Path("/local/app")):
            env = web_api.managed_uv_environment()

        self.assertEqual(env["UV_DATA_DIR"], str(Path("/local/app/uv")))
        self.assertEqual(env["UV_PYTHON_INSTALL_DIR"], str(Path("/local/app/uv/python")))

    def test_video_preview_uses_a_bounded_real_frame(self):
        with TemporaryDirectory() as directory:
            video = Path(directory) / "source clip.mp4"
            video.write_bytes(b"video data")
            payload = {
                "width": 1920,
                "height": 1080,
                "fps": 24,
                "frames": 24,
                "duration_seconds": 1,
                "codec": "h264",
                "container": "mp4",
                "size_bytes": video.stat().st_size,
                "frame": "encoded-jpeg",
            }
            with (
                patch.object(web_api, "PYTHON", sys.executable),
                patch.object(
                    web_api.subprocess,
                    "run",
                    return_value=CompletedProcess([], 0, json.dumps(payload), ""),
                ) as run,
            ):
                result = web_api.inspect_video(video, include_frame=True)

            script = run.call_args.args[0][2]
            self.assertEqual(result["size_bytes"], video.stat().st_size)
            self.assertIn("cv2.resize", script)
            self.assertIn("1280 / width", script)
            self.assertIn("720 / height", script)

    def test_managed_files_cannot_be_deleted_while_render_is_stopping(self):
        with patch.dict(web_api.render, {"status": "stopping"}):
            with self.assertRaisesRegex(ValueError, "active work"):
                web_api.delete_managed_data("models")

    def test_managed_python_cannot_be_deleted_during_model_storage_setup(self):
        with patch.dict(web_api.storage_job, {"status": "installing"}):
            with self.assertRaisesRegex(ValueError, "active work"):
                web_api.delete_managed_data("python")

    def test_version_tuple_ignores_release_prefix_and_suffix(self):
        self.assertEqual(web_api.version_tuple("v2.4.30-beta.1"), (2, 4, 30))

    def test_update_check_selects_windows_installer_and_requires_digest(self):
        with (
            patch.object(web_api, "APP_VERSION", "2.4.29"),
            patch.object(web_api, "fetch_latest_release", return_value=(_release(), "gh")),
        ):
            result = web_api.check_github_update()

        self.assertTrue(result["update_available"])
        self.assertEqual(result["latest_version"], "2.4.30")
        self.assertEqual(result["access_method"], "gh")
        self.assertEqual(result["installer_sha256"], "a" * 64)
        self.assertTrue(result["installer_name"].endswith("Windows-Installer.exe"))

    def test_update_without_sha256_is_not_installable(self):
        with (
            patch.object(web_api, "APP_VERSION", "2.4.29"),
            patch.object(web_api, "fetch_latest_release", return_value=(_release(""), None)),
        ):
            result = web_api.check_github_update()

        self.assertFalse(result["update_available"])
        self.assertIsNone(result["installer_sha256"])
        self.assertIn("checksum", result["detail"])

    def test_cpu_backend_remains_available_on_unsupported_cuda_arch(self):
        self.assertEqual(
            web_api.available_backends_from_probe(True, ["pytorch"]),
            ["pytorch"],
        )

    def test_missing_opencv_blocks_backends_that_require_it(self):
        self.assertEqual(
            web_api.available_backends_from_probe(False, ["pytorch", "ncnn"]),
            [],
        )

    def test_render_backend_gate_uses_selected_backend(self):
        capabilities = {
            "opencv_available": True,
            "available_backends": ["ncnn", "tensorrt"],
        }
        self.assertTrue(web_api.selected_backend_is_available(capabilities, "ncnn"))
        self.assertTrue(web_api.selected_backend_is_available(capabilities, "tensorrt"))
        self.assertFalse(web_api.selected_backend_is_available(capabilities, "pytorch"))
        self.assertTrue(
            web_api.selected_backend_is_available(
                {"opencv_available": True, "available_backends": ["pytorch"]}, "auto"
            )
        )

    def test_service_port_collision_exits_without_unhandled_exception(self):
        with TemporaryDirectory() as directory:
            with (
                patch.object(web_api, "APP_DATA", Path(directory)),
                patch.object(
                    web_api,
                    "SingleOwnerHTTPServer",
                    side_effect=OSError(10048, "address already in use"),
                ),
                patch.object(web_api, "begin_model_storage_setup") as setup_models,
                patch.dict(
                    "os.environ",
                    {
                        "RVE_AUTO_SETUP_MODELS": "1",
                        "RVE_AUTO_SETUP_RUNTIME": "0",
                        "RVE_MODEL_STORAGE": "wsl",
                    },
                ),
            ):
                self.assertIsNone(web_api.run_server())
                setup_models.assert_not_called()


if __name__ == "__main__":
    unittest.main()
