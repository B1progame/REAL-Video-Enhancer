# Changelog

## 2.4.50 — 2026-11-23

- Keep the no-interpreter Python check from invoking an empty executable path, and explicitly report external Python as different from the app-managed runtime.
- Add regression coverage for missing and external Python interpreter states.

## 2.4.49 — 2026-11-23

- Distinguish app-managed Python from a system Python so Runtime and Python do not report an unmanaged interpreter as ready.
- Explain when setup will create the managed environment, while keeping the detected external interpreter visible for diagnostics.

## 2.4.48 — 2026-11-23

- Report missing or broken managed Python cleanly instead of launching an empty interpreter path and showing a Windows parameter error.
- Make Runtime reflect failed or cancelled setup even when system Python happens to report installed backends, and expose the last installer output directly in the page.
- Add a clear retry action on the Python page, improve compact sidebar selection contrast, and give the custom accent picker its own labeled control.

## 2.4.47 — 2026-11-23

- Use consistent truncated, copyable path rows across Process, Downloads, Runtime, and Python so long paths stay inside their cards.

## 2.4.46 — 2026-11-23

- Stretch Runtime hardware details across their cards and keep long Python paths readable without pushing the Copy action offscreen.

## 2.4.45 — 2026-11-23

- Keep the two-column Home hero and three status cards at the 1184×950 desktop layout validated in the release review, while retaining narrow-window reflow.
- Build releases into versioned staging directories so a running app cannot lock files needed by the next package build.

## 2.4.44 — 2026-11-23

- Reflow the Home hero and status cards earlier to prevent cramped layouts at common window sizes, and let the page header wrap cleanly.
- Keep model download actions disabled until the engine is connected and explain the waiting state.

## 2.4.43 — 2026-11-23

- Keep the Home hero and status cards side by side when the available width supports them, so useful dashboard content stays visible without scrolling at common desktop sizes.

## 2.4.42 — 2026-11-23

- Prevent Home content from overflowing after window or display-DPI changes by stacking the hero and status cards at compact sizes.

## 2.4.41 — 2026-11-23

- Keep the Process workspace in one column until the window is wide enough for both cards, preventing clipped controls on compact windows.

## 2.4.40 — 2026-11-23

- Update from Settings now installs the checksum-verified download automatically in silent mode and reopens the app when it finishes.
- Keep existing settings, Python runtime, models, and WSL data during an app update; the regular uninstaller still offers a separate data cleanup choice.

## 2.4.39 — 2026-11-23

- Fix Windows taskbar identity and installer shortcuts so the app consistently displays the REAL Video Enhancer icon.

## 2.4.38 — 2026-11-23

- Refined all six native Rust screens with a shared graphite theme, clearer hierarchy, responsive layouts, and a consistent Lucide icon set.
- Added a real source-frame preview with video metadata and a searchable model catalog with live installed counts.
- Improved the Process empty state, render readiness feedback, reduced-motion behavior, and keyboard-operable enhancement switches.
- Embedded the branded Windows executable icon and matched the native Windows title bar to the selected appearance.

## 2.4.37 — 2026-11-23

- Cancel now terminates the active WSL model-library installer process directly.
- Prevent managed Python/model deletion while a render is stopping.

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
