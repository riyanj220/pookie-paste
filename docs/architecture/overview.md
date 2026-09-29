# Architecture Overview

Pookie Paste is a native, lightweight clipboard history application for Linux inspired by Windows Clipboard History. It enables users to browse clipboard history, preview text and images, and immediately restore and paste selections back into the active application.

The project is designed to be minimal, native, fast, and reliable across X11 and diverse Wayland environments (KDE Plasma, Sway, Hyprland).

---

## Process Architecture

Pookie Paste splits responsibilities between two separate processes:

1. **Persistent Daemon (`pookie-paste`)**: A long-running background service that owns clipboard monitoring, persistence, image file storage, platform backends (focus capture, paste injection, shortcut listener), and a local Unix IPC server.
2. **Ephemeral Popup UI (`pookie-paste-ui`)**: A short-lived graphical window built with `eframe`/`egui`. It contains no durable application state and communicates with the daemon exclusively over local Unix domain socket IPC.

```mermaid
flowchart TD
    Desktop["Desktop Session & Applications"]
    Clipboard["System Clipboard (X11 / Wayland)"]
    Daemon["Persistent Daemon (pookie-paste)"]
    Backends["Platform Backends (Focus, Paste, Shortcuts)"]
    Storage["History Storage (SQLite + Images)"]
    IPC["Unix Domain Socket IPC"]
    UI["Ephemeral Popup UI (pookie-paste-ui)"]

    Desktop <--> Clipboard
    Clipboard <--> Daemon
    Daemon <--> Backends
    Backends <--> Desktop
    Daemon <--> Storage
    Daemon <--> IPC
    IPC <--> UI
```

---

## Architectural Boundaries

The codebase enforces strict boundaries so that platform-specific windowing and compositor quirks do not leak into history, storage, or UI presentation:

* **Persistent vs. Ephemeral State**:
  The UI does not own durable application state that must survive popup termination. All persistent clipboard history, configuration state, platform handles, and active watcher sessions are owned strictly by the daemon. The UI retains only transient view state (such as active search queries, navigation indices, and in-memory thumbnail textures).
* **Storage Separation**:
  Structured history metadata and text entries reside in a local SQLite database, whereas raw image bytes are stored as canonical PNG files on the filesystem. Database rows reference images via application-relative paths (`images/<uuid>.png`), keeping stored data independent of user home directories.
* **IPC Decoupling**:
  The UI does not access SQLite or clipboard APIs directly. History metadata comes through daemon IPC. Raw image payloads never traverse the socket wire; instead, the UI resolves and loads image preview files locally from the daemon-managed image store on the filesystem using the application-relative path supplied in history metadata (`images/<uuid>.png`), decoding and caching thumbnail textures in GPU memory on demand.
* **Platform Abstraction**:
  Desktop-specific behavior is isolated behind unified Rust traits:
  * `ClipboardBackend` and `ClipboardWatcher` for clipboard I/O and change notifications.
  * `FocusBackend` for capturing and restoring target application focus.
  * `PasteBackend` for synthesizing paste keystrokes.
  * `ShortcutBackend` for registering global hotkeys.

---

## Core Subsystems

Pookie Paste is structured into four main functional subsystems, each documented in detail in its respective section:

### 1. Clipboard Management
Monitors the system clipboard for new entries, applies content limits and normalization, and canonicalizes all incoming image formats (PNG, JPEG, WebP, BMP, GIF) into canonical PNG-encoded RGBA8 bytes. Includes self-write suppression so that Pookie-initiated clipboard writes do not generate duplicate history items.
*Detailed guide: [Clipboard Overview](../clipboard/overview.md)*

### 2. History & Storage
Coordinates dual-layer persistence: SQLite for index metadata and text content, and a filesystem store for canonical PNG images. Enforces history limits, deduplication, item pinning, and startup orphan image reconciliation.
*Detailed guide: [Clipboard Overview](../clipboard/overview.md)*

### 3. Activation & Direct Paste
Governs what happens when a history item is selected. It executes a strict chronological sequence: content retrieval, clipboard writeback, history timestamp promotion, target window focus restoration, authoritative target confirmation, and synthetic paste injection.
*Detailed guide: [Activation Overview](../activation/overview.md)*

### 4. Global Shortcuts
Provides cross-desktop shortcut handling across three distinct paradigms: native key grabs (X11), desktop portals (KDE Plasma), and compositor-managed keybindings (Sway, Hyprland). Supports live configuration reloads, observational status rechecks, and conflict detection.
*Detailed guide: [Shortcuts Overview](../shortcuts/overview.md)*

---

## Crate Organization

The repository is organized as a Cargo workspace with distinct crate boundaries:

| Crate | Responsibility |
| --- | --- |
| [`crates/pookie-core`](../../crates/pookie-core) | Core domain types, text normalizer, content hasher (SHA-256), and clipboard policy limits. |
| [`crates/pookie-clipboard`](../../crates/pookie-clipboard) | Clipboard backend abstractions, X11/Wayland readers and watchers, and canonical image codec. |
| [`crates/storage`](../../crates/storage) | SQLite repository, schema migrations, and atomic filesystem `ImageStore`. |
| [`crates/history`](../../crates/history) | High-level `ClipboardHistoryService`, eviction policy, deduplication, and item pinning. |
| [`crates/ipc`](../../crates/ipc) | Unix domain socket transport, newline JSON framing codec, and typed protocol messages. |
| [`crates/daemon`](../../crates/daemon) | Main background process, platform resolvers, focus/paste/shortcut backends, and IPC request handler. |
| [`crates/ui`](../../crates/ui) | Ephemeral popup GUI, egui rendering, input navigation, thumbnail cache, and shortcut view. |

---

## Next Steps

To understand the runtime lifecycle, process model, and filesystem conventions, see:
* [Runtime Model & Process Lifecycle](runtime-model.md)
