# Roadmap

This roadmap describes the direction of the B1progame fork. The upstream Python engine remains the source of video processing behavior; the Windows interface is being rebuilt as a native Rust/egui desktop app. Items may change as the application is exercised on real systems.

## Available now

- Native Rust desktop shell with Home, Process, Downloads, Runtime, Python, and Settings pages.
- Existing Python rendering engine and runtime setup connected through a local-only service.
- Windows or WSL Ubuntu model storage selection and guided model preparation.
- Appearance selection, locally persisted accent color, and reduced-motion preference.
- GitHub release check, verified installer download, and installer launch from Settings. Private release access uses the signed-in GitHub CLI.
- Inno Setup 7 Windows installer and an uninstaller choice to remove managed models and runtime data.

## In progress

- Complete the shared visual system and tune every page against actual window-size and DPI scenarios.
- Improve Process workflow validation and make selected models, output details, and render readiness clearer.
- Add rendered screenshots and interaction checks to the release review process.
- Keep setup and update failures actionable, including private repository authentication and WSL startup states.

## Planned

- Finish and validate the preserved AnimeScripter integration as a separate effort. It is unfinished and is not part of the supported enhancement flow yet.
- Continue reducing startup and idle resource use while keeping the existing Python engine behavior.
- Expand keyboard navigation and screen-reader review across the six pages.
- Improve recovery and retry controls for model and runtime operations where the engine supports them.

## Not planned for this roadmap

The project will remain a native Rust/egui Windows app with the Python video engine. Replacing it with a browser interface or rewriting inference in Rust is outside the current direction.
