# Changelog

## 2.4.36 — 2026-11-23

- Added cancellation for active render, model, runtime, WSL storage, and app update work, with a cancel-and-quit action when closing during work.
- Added confirmed Settings actions to remove the managed Python runtime and model weights from the selected storage location.
- Switched managed Python and package installation to an app-local, checksum-verified uv runtime.

## 2.4.35 — 2026-11-22

- Open directly into the Process workspace on first launch and show a clear first-run setup prompt there.
- Arrange Settings in adaptive columns on wider windows and keep it single-column at compact sizes.

## 2.4.34 — 2026-11-22

- Closing the window during a render, model download, runtime setup, storage setup, or app update now offers to keep the work running in the system tray.
- Added tray actions to reopen the app and request a clean quit after active work finishes.
- Quitting while idle shuts down the local engine, and installer completion launches the updated app.

## 2.4.33 — 2026-11-22

- Use an OS-assigned loopback port to avoid Windows port exclusions and existing listeners.
- Remember the service port so another desktop launch can reconnect to the running engine.
- Keep the fixed-port probe for compatibility with older running engine builds.

## 2.4.32 — 2026-11-22

- Prevented the packaged engine from showing an unhandled exception when its loopback API port is already in use.
- Reuse a healthy local RVE engine when the desktop app is opened a second time.
- Check port ownership before starting WSL model setup, preventing duplicate setup work.
- Added native app startup diagnostics for occupied or unavailable service ports.
- Refined the Rust app shell and added a video file drop target to Process.
- Preserved saved render settings and support for the selected inference backend.
