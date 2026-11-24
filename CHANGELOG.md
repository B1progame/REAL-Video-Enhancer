# Changelog

## 2.4.67 — 2026-11-24

- Rework the Process page into a responsive two-column video workspace with source and model selection above output and advanced render settings.
- Bring backend, precision, frame multiplier, scale override, tiling, GPU, HDR, crop, overwrite, and preview controls directly into the processing workflow.
- Refresh the dark theme with a deeper blue-black canvas, clearer surface contrast, and the existing user-selectable accent colors.

## 2.4.66 — 2026-11-24

- Delete the complete app-managed Python environment, including uv-installed CPython builds, the virtual environment, and package cache.
- Detect the installed WSL distribution and its real home directory for model deletion and setup instead of relying on a misspelled distro and a hard-coded Linux username.
- Show delete progress and success or failure in Settings, refresh Python, model, and storage state after removal, and reset stale ready statuses.
- Correct the WSL distribution used by the installer's optional full-data uninstall cleanup.

## 2.4.65 — 2026-11-24

- Populate Runtime hardware details from the managed PyTorch probe so the graphics device and CUDA runtime are reported accurately.
- Clearly identify CPU-only runtime setups instead of showing a missing graphics device.
- Pair operating system with graphics device, and CUDA with available backends, in the hardware summary.
- Normalize saved processing backend names, prevent selecting backends that are not installed, and link blocked renders directly to Runtime setup.

## 2.4.64 — 2026-11-24

- Redesign the native app's shared visual system with a calmer, more refined light and dark palette, stronger typography, roomier cards, and clearer controls.
- Keep the labeled sidebar visible at normal desktop sizes and improve navigation, logo, and page-header proportions.
- Give source and home video previews more space, making the footage the focus of the workflow.

## 2.4.63 — 2026-11-24

- Handle empty, malformed, or non-object responses from the GitHub API and CLI without leaking a `json.loads(None)` exception into Settings.
- Add regression coverage for an empty GitHub CLI response.

## 2.4.62 — 2026-11-24

- Simplify the ready-state model storage summary so the WSL path is shown once in its copyable path field instead of wrapping inside the status message.

## 2.4.61 — 2026-11-23

- Add an opt-in, real-time render preview to the Process workspace using frames produced by the existing RVE engine.
- Poll preview frames only during active RVE rendering and display the last processed frame when a render completes.
- Expose the existing preview preference in Settings and enable it by default for native app installs.
- Add a focused test for preview polling states.

## 2.4.60 — 2026-11-23

- Disable uv configuration discovery during managed runtime installs so user or project index settings cannot redirect dependencies away from PyPI.
- Add regression coverage for uv config isolation.

## 2.4.59 — 2026-11-23

- Keep ordinary runtime dependencies such as `requests` and PyTorch prerequisites on PyPI, separate from PyTorch wheel indexes.
- Install pinned PyTorch wheels alone from the official wheel index so their dependencies cannot be resolved from the wrong index.
- Clear inherited `UV_*` and `PIP_*` index overrides during managed runtime setup.
- Add regression coverage for index isolation and deterministic package sources.

## 2.4.58 — 2026-11-23

- Replace the tiny Windows native caption icon with a branded, resizable 42 px custom title bar and a crisp 24 px logo.
- Add minimize, maximize/restore, close, drag-to-move, and double-click-to-maximize controls while preserving close-while-busy handling.
- Keep the title bar in sync with the selected light or dark appearance.

## 2.4.56 — 2026-11-23

- Retry Windows Python and runtime process launches without optional startup metadata when a launch context returns WinError 87.
- Ignore stale `RVE_PYTHON` environment overrides in packaged builds so they always use the app-managed interpreter.
- Distinguish a present but unlaunchable Python interpreter from a runtime that is not installed, and offer a direct repair action.
- Add regression coverage for packaged interpreter selection and Windows process launch fallback.

## 2.4.55 — 2026-11-23

- Make copyable paths and searchable model pickers fit the available window width, switching long paths to a stacked layout on narrow panels.
- Reflow manual input and output path controls when the Process workspace is compact.
- Switch Runtime hardware details to one column when two columns would make values cramped.
- Add layout boundary coverage for the compact Runtime card.

## 2.4.54 — 2026-11-23

- Launch the app from Inno Setup using the original user's token so uv can create managed Python version junctions without weakening Setup's RedirectionGuard protection.
- Keep packaged builds bound to their app-managed Python instead of probing unrelated PATH aliases that can fail with WinError 87.
- Refresh Python and backend status after runtime setup completes, and replace stale setup errors with live progress while retrying.
- Clarify pending, missing, and unavailable interpreter states on the Python page.
- Improve responsive model pickers and shared status styling, and fix trimmed model-search behavior.

## 2.4.53 — 2026-11-23

- Clear an incomplete managed Python environment before recreating it, so setup recovers cleanly from folders left by older runtime setup attempts.
- Keep uv's Python toolchains and version links under Local AppData to avoid OneDrive Files On-Demand mount-point failures on Windows.
- Show the short failure reason once, with command diagnostics in the setup log instead of duplicating them in the page.
- Add regression coverage for retry paths and uv's local data directories.

## 2.4.52 — 2026-11-23

- Create the managed Python virtual environment at its root directory so uv places the interpreter where the app expects it.
- Treat failed or non-zero Python probes as unavailable, show an actionable error, and require app-managed Python before Runtime reports ready.
- Include the last subprocess output in setup failures to expose the actual uv error instead of only its exit code.

## 2.4.51 — 2026-11-23

- Keep background status polling from clearing the in-flight action state, and reject duplicate POST actions until the current request completes.
- Add a regression check for busy-state handling across poll and action responses.

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
