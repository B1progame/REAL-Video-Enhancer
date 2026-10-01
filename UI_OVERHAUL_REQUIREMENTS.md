# UI Overhaul Requirements (High Priority)

This project needs a full GUI redesign focused on usability, consistency, and performance.

## Core Goal
- Make the interface feel modern, fast, and easy to understand at first use.
- Keep all existing features, but improve layout, clarity, and interaction flow.

## Current Problems To Fix
- Layout crowding in the left workflow panel.
- Overlap/clipping issues when window scales or DPI changes.
- Inconsistent spacing, typography, and control sizes across tabs.
- Weak hierarchy: important actions do not stand out enough.
- Switch/update lag when changing pages or modes.
- Settings page has oversized label blocks and too much empty space.
- Preview area and control area can become visually unbalanced.

## Design Requirements
- Strong visual hierarchy:
  - Primary action buttons must be clearly identifiable.
  - Section headers should be visible and consistent.
  - Related controls should be grouped in cards/panels.
- Consistent spacing scale:
  - Reuse fixed spacing tokens (e.g., 6/10/14/20 px).
  - Avoid arbitrary per-widget margins unless required.
- Responsive layout behavior:
  - No overlap at common resolutions and DPI scaling.
  - Process controls and preview must rebalance on resize.
- Better readability:
  - Increase text contrast where currently low.
  - Use predictable typography sizes and weights for labels/body/info.
- Navigation simplification:
  - Keep process-first workflow.
  - Remove dead/unused nav paths and stale page logic.

## Functional UX Requirements
- Startup should land on process workflow directly.
- Page switching should always refresh reliably with no stale paints.
- Render states (idle/building/rendering/paused) must have clear status feedback.
- Output/queue actions should be obvious and discoverable.
- Tooltips/help icons should be clear but non-intrusive.

## Performance Requirements
- Avoid expensive repaint or animation chains during page switching.
- Remove unnecessary graphics effects after animation completion.
- Keep UI updates incremental; do not force full-window repaint unless needed.

## Icon & Branding Requirements
- Use `logo-v3` consistently for:
  - App window icon.
  - Installer icon.
  - Executable icon.
  - Any in-app branding/icon popup usage.
- Remove old branding assets/references once migration is complete.

## Code Quality / Cleanup Requirements
- Remove unused UI code paths and dead files.
- Keep only required runtime assets in `icons/` and `resources.qrc`.
- Avoid duplicate logic for layout styling.
- Document major UI layout decisions in code comments where needed.

## Verification Checklist
- Launch app at 100%, 125%, 150% DPI without overlap.
- Resize window from minimum to fullscreen without clipping.
- Switch between Process / Settings / Downloads repeatedly without lag.
- Start a render and confirm preview + controls remain aligned.
- Build installer and confirm icon/branding are all `logo-v3`.

## Next Iteration Scope
1. Rebuild process workflow with explicit sections and better card layout.
2. Refactor settings rows to compact, two-column adaptive layout.
3. Standardize typography and spacing tokens globally.
4. Add lightweight UI performance profiling for tab/page transitions.
