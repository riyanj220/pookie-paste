# Changelog

All notable changes to Pookie Paste are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

Development after the `0.2.0` release will be documented here.

---

## [0.2.0] - TBD

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