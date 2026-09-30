# Changelog

All notable changes to Pookie Paste are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

Development after the `0.3.0` release will be documented here.

---

## [0.3.0] - 2026-09-30

This release introduces first-class cross-desktop global shortcut support—expanding full support to Sway and Hyprland alongside X11 and KDE Plasma Wayland—powered by unified cross-desktop shortcut management with platform-specific ownership semantics. It also hardens Linux desktop integration with dedicated application icon assets, standardized window identity, shortcut-aware installer onboarding, a remote uninstaller, and modernized lifecycle testing.

### Added

- First-class Sway global shortcut integration with live compositor configuration inspection and variable expansion
- First-class Hyprland global shortcut integration supporting both modern Lua bindings and classic Hyprlang directives
- Focus restoration and direct paste support for Sway and Hyprland
- Unified shortcut capability and status model across native key grabs, desktop portals, and compositor-managed keybindings
- Machine-readable shortcut status via `pookie-paste --shortcut-status --porcelain`
- Dedicated shortcut configuration interface via `pookie-paste --shortcut-setup` with live conflict detection and copyable configuration snippets
- Dynamic runtime shortcut recheck, rebind, and configuration reload
- Dedicated application icon theme across standard resolutions, embedded window icon, and canonical `io.github.riyanj220.PookiePaste` desktop identity
- Remote uninstaller bootstrapper (`uninstall.sh`) supporting standard uninstall and full `--purge`
- Automated installation lifecycle smoke testing covering fresh install, persistence, reinstallation, and purge flows

### Changed

- Shortcut ownership is explicitly platform-specific: native key grabs on X11, authoritative portal assignment on KDE Plasma, and compositor-managed keybindings on Sway and Hyprland
- Configuration in `config.toml` represents user intent and avoids silently modifying compositor configs or external portal registrations
- Installer onboarding inspects live daemon status to guide users based on actual shortcut availability instead of assuming `Super+V` is active
- Application desktop launcher now opens the popup directly (`pookie-paste --toggle`) and starts the background daemon automatically if needed
- Popup window adopts an explicit dismissal policy on Sway and Hyprland while retaining focus-loss dismissal on X11 and KDE Plasma
- Standard uninstallation preserves user databases, images, and configuration, while `--purge` completely wipes all user data
- Release archives now bundle application icons alongside binaries, desktop entries, and integration scripts
- Reworked architecture, shortcut, platform-support, installer, and release-testing documentation to match current runtime behavior

### Fixed

- KDE Plasma portal integration falsely reporting active status when no shortcut was assigned in System Settings
- Sway shortcut status requiring a daemon restart to reflect external compositor configuration changes
- Silent shortcut collisions with existing compositor keybindings
- Launchers and taskbars displaying generic executable placeholder icons instead of official Pookie Paste artwork
- Direct paste attempting keystroke injection when target focus restoration could not be safely confirmed
- Installation smoke tests triggering interactive GUI shortcut setup windows during automated runs

---

## [0.2.1] - 2026-09-20

This release introduces clipboard history management features—pinning, per-item deletion, and full history clearing—along with popup UX refinements, custom vector UI rendering, smart window positioning, and modular UI architecture.

### Added

- Clipboard item pinning via card context menu to keep important clips pinned to the top of the history
- Persistent `pinned_at` tracking in SQLite with database schema migration
- Pinned items protected from automatic history capacity eviction
- Preservation of pin state when re-copying existing items
- Per-item deletion via card context menu, removing database records and deleting associated stored image files
- Header "Clear" action button to clear all history records and stored image files
- Dedicated history card context menu with `Pin`/`Unpin` and `Delete` actions
- Custom vector-drawn 3-dot overflow icon and pinned pushpin indicator using `egui::Painter`, eliminating font glyph dependencies and missing-glyph (`□`) rendering issues across Linux distributions
- Smart popup window positioning near the active application window on X11
- IPC protocol extensions for item pin states, `TogglePin`, `DeleteItem`, and `ClearHistory` requests

### Changed

- Separated history card activation from action button clicks to completely eliminate accidental paste triggers
- Context menu automatically closes on outside clicks, `Escape`, or item selection
- Header "Clear" button is styled subtly to avoid intrusive destructive styling and only displays when history contains items
- Refactored `crates/ui/src/main.rs` into modular subcomponents (`app`, `history`, `actions`, `controls`, `header`, `rows`) preserving all behavior and tests

---

## [0.2.0] - 2026-09-18

This release adds image clipboard history and completes the first mixed text/image Pookie Paste experience on X11 and KDE Plasma Wayland.

### Added

- Image clipboard history alongside existing text history
- Mixed text and image rows in the `Super+V` popup
- Image thumbnail rendering with preserved aspect ratios
- Canonical PNG image representation for consistent storage and deduplication
- PNG, JPEG/JPG, WebP, BMP, and GIF input support
- Filesystem-backed image persistence under the Pookie Paste data directory
- Image activation and clipboard writeback on X11
- Image activation and clipboard writeback on Wayland
- Image-aware self-write suppression
- Image history lifecycle handling for:
  - duplicate promotion
  - history-limit eviction
  - deletion
  - clearing history
  - startup orphan cleanup
- Mixed-content regression and image lifecycle test coverage
- Image metadata/reference support in the IPC protocol without sending raw image bytes through IPC

### Changed

- Clipboard abstractions are now content-aware and support both text and images
- X11 clipboard monitoring now handles mixed text/image content
- Wayland clipboard monitoring now supports image MIME offers through:
  - `ext-data-control-v1`
  - `wlr-data-control-v1`
- Wayland MIME selection prefers real image data when applications expose both image and text fallback formats
- History persistence now stores:
  - text directly in SQLite
  - image metadata in SQLite
  - canonical image files under `images/<uuid>.png`
- Popup UI now loads short-lived image thumbnails locally from stored image files
- Activation flow now reconstructs either text or image content before clipboard writeback
- Project architecture, development, roadmap, README, and release-testing documentation updated to reflect the current implementation

### Fixed

- Wayland clipboard MIME state leaking between consecutive data offers
- Spurious empty `image/png` payload errors when transitioning from image clipboard content to text
- X11 popup focus timeout starting before the native popup window was actually ready
- Popup UI process remaining alive after asynchronous activation completed
- Repeated `Super+V` activation being blocked by a stale popup process
- Mixed-content activation creating duplicate history entries through self-generated clipboard events

---

## [0.1.1] - 2026-09-16

Release-infrastructure validation release used to exercise the public installation and distribution workflow.

### Added

- Prebuilt Linux release packaging pipeline
- GitHub release installation support
- Version-specific installation support
- SHA256 verification for downloaded release artifacts
- Source-install smoke testing
- KDE Plasma Wayland validation tooling
- Automated KWin focus-helper installation as part of KDE setup
- Release packaging and runtime validation documentation

### Changed

- Installer can update an existing Pookie Paste installation while preserving user data
- Release artifacts include the daemon, popup UI, desktop integration, autostart entry, KWin helper, license, README, and release version metadata

---

## [0.1.0]

Initial public Pookie Paste release.

### Added

- Background clipboard daemon
- Persistent text clipboard history
- SQLite-backed history storage
- `Super+V` global shortcut
- Interactive clipboard history popup
- Keyboard and mouse history selection
- Clipboard activation and writeback
- X11 clipboard monitoring
- Wayland clipboard monitoring
- X11 focus restoration and direct paste
- KDE Plasma Wayland focus integration
- Portal/EIS direct paste on supported KDE Wayland sessions
- Clipboard-only fallback when safe direct paste is unavailable
- Unix-domain socket IPC between daemon and UI
- Self-generated clipboard-event suppression
- Automatic startup after login
- User-local installation
- Update and uninstall tooling
- Optional full application-data purge