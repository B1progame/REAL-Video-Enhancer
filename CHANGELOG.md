# Changelog

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
