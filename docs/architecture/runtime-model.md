# Runtime Model & Process Lifecycle

This document describes the operational lifecycle, process interaction, singleton management, and filesystem conventions of Pookie Paste.

---

## Process Overview

Pookie Paste operates using a two-tier process architecture:

```mermaid
sequenceDiagram
    participant User as User / Hotkey
    participant Daemon as Daemon (pookie-paste)
    participant UI as Popup UI (pookie-paste-ui)
    participant Compositor as Window Manager / Compositor

    User->>Daemon: Global Hotkey or CLI (--toggle)
    Daemon->>Daemon: Atomic claim via UiLauncher
    Daemon->>UI: Spawn process
    UI->>Daemon: IPC CaptureFocusTarget
    Daemon->>Compositor: Query active window identity (FocusBackend)
    Daemon-->>UI: FocusTarget
    UI->>UI: Map window (anchored on X11, compositor-placed on Wayland)
    UI->>Daemon: IPC GetHistory
    Daemon-->>UI: HistoryItem metadata
    User->>UI: Select item (Click / Enter)
    UI->>Daemon: IPC ActivateItem(id, target)
    UI->>UI: Hide window & await outcome
    Daemon->>Compositor: Restore focus & confirm active target
    Daemon->>Compositor: Synthesize Ctrl+V (if target confirmed)
    UI->>UI: Close process on successful activation
```

### State Ownership Precision
The UI does not own durable application state that must survive popup termination. Persistent clipboard history, image files, configuration state, platform handles, and background watchers are owned strictly by the daemon. While running, the UI maintains only transient runtime view state, such as:
* The active search and filter query.
* Current keyboard navigation index.
* Active view mode (clipboard history vs. shortcut setup).
* An in-memory GPU texture cache ([`ImageThumbnailCache`](../../crates/ui/src/image_thumbnail.rs)) for displayed image items.

---

## CLI Modes & Entry Points

The `pookie-paste` binary serves as both the persistent background daemon and the command-line control client depending on passed flags:

| Command | Action | Implementation |
| --- | --- | --- |
| `pookie-paste` | Starts the persistent background daemon service. | [`crates/daemon/src/main.rs`](../../crates/daemon/src/main.rs) |
| `pookie-paste -t, --toggle` | Requests popup display through the daemon while preserving single-instance UI behavior. | [`crates/daemon/src/cli.rs`](../../crates/daemon/src/cli.rs) |
| `pookie-paste -r, --reload` | Requests strict reload of `config.toml` in the running daemon. | [`crates/daemon/src/cli.rs`](../../crates/daemon/src/cli.rs) |
| `pookie-paste --shortcut-status` | Authoritatively queries and formats live global shortcut configuration and runtime binding status. | [`crates/daemon/src/cli.rs`](../../crates/daemon/src/cli.rs) |
| `pookie-paste -h, --help` | Displays usage instructions and available options. | [`crates/daemon/src/cli.rs`](../../crates/daemon/src/cli.rs) |
| `pookie-paste -V, --version` | Prints the application version. | [`crates/daemon/src/cli.rs`](../../crates/daemon/src/cli.rs) |

> [!NOTE]
> `--toggle` is named historically but does not implement a true visibility toggle (i.e. it does not close an already visible popup). If the popup is already running, the daemon preserves the existing window and returns `UiLaunchOutcome::AlreadyRunning`. On X11 and KDE Plasma Wayland, an open popup closes when focus is transferred away from the popup under its dismissal policy, while on Sway and Hyprland, bare focus loss is ignored to accommodate compositor pointer-driven focus transitions.

---

## Daemon Lifecycle

### 1. Singleton Enforcement & Socket Binding
On launch, the daemon attempts to bind the Unix domain socket at `$XDG_RUNTIME_DIR/pookie-paste/pookie.sock`. Before binding:
1. It inspects whether the path already exists.
2. It attempts a test connection (`std::os::unix::net::UnixStream::connect`).
3. If the connection succeeds, another daemon instance is already active; the new process logs an informational message and exits with status 0.
4. If the connection is refused, the socket file is stale from a previous unclean shutdown; the daemon removes the file and proceeds to bind.

For complete wire protocol and stale socket cleanup mechanics, see [IPC Overview](../ipc/overview.md).

### 2. Startup Sequence
When started, the daemon executes its initialization in strict chronological order:
1. **CLI Action Resolution**: Evaluates command-line arguments (`cli::parse_args()`). If arguments request client action (such as `--toggle` or `--reload`), the process executes the client command over IPC and exits; otherwise, it proceeds as the background daemon.
2. **Logging Initialization**: Configures structured tracing logging (`logging::init_logging()`).
3. **Singleton Socket Binding**: Probes and binds the local Unix domain socket (`$XDG_RUNTIME_DIR/pookie-paste/pookie.sock`). If an active daemon is detected, the new process exits cleanly with status 0; stale socket files are automatically unlinked.
4. **App Data Paths & Storage Initialization**: Ensures `$XDG_DATA_HOME/pookie-paste/` exists, connects to SQLite (`pookie-paste.db`), and applies schema migrations (`Database::new()`).
5. **Storage & History Construction**: Instantiates `StorageRepository`, `ImageStore`, and `ClipboardHistoryService`.
6. **Startup Image Store Reconciliation**: `reconcile_image_store()` runs synchronously before normal service activation: SQLite is authoritative; every image file referenced by an active database row is strictly preserved, while unreferenced PNG files and leftover temporary files are unlinked.
7. **Platform Environment Audit**: Inspects `XDG_SESSION_TYPE` and `XDG_CURRENT_DESKTOP` to determine session capabilities.
8. **Clipboard Backend & Watcher**: Resolves the platform clipboard backend and starts the background clipboard watcher (event-driven via XFixes on X11 with generation fallback polling, or data-control protocols on Wayland).
9. **Focus & Paste Services**: Resolves the platform focus backend and paste backend. Direct paste capability is enabled only if focus restoration and confirmation can be guaranteed on the active desktop.
10. **Shortcut Listener & Config Bootstrap**: Ensures `$XDG_CONFIG_HOME/pookie-paste/config.toml` exists, constructs `ReloadCoordinator`, and initializes the background shortcut worker.
11. **Activation Service & UI Launcher**: Constructs `ClipboardActivationService` and `UiLauncher`.
12. **IPC Server Task & Ready Log**: Pins the IPC server listener future (`ipc_server::run()`) and logs `"Pookie daemon running"`.
13. **Background Legacy Migration Spawn**: Spawns `migrate_legacy_image_identities()` as an independent background task via `tokio::spawn`. Background migration adds no synchronous image-decoding work to the existing startup critical path and runs concurrently with normal daemon operation.
14. **Main Event Loop**: Enters the asynchronous event loop (`tokio::select!`), multiplexing IPC connections, clipboard watcher notifications, global shortcut triggers, and OS signals.

### 3. Background Legacy Image Migration Lifecycle
For databases containing legacy image rows created before `rgba-v1` pixel identities (where rows recorded raw 64-hex SHA-256 digests of PNG bytes), the daemon runs an asynchronous migration task:
* **Concurrence with Normal Operation**: Runs in a spawned Tokio task (`tokio::spawn`) concurrently with the main daemon event loop. Background migration adds no synchronous image-decoding work to the existing startup critical path and runs concurrently with normal daemon operation.
* **Sequential Execution**: Candidate rows are processed sequentially (`concurrency = 1`). Stored PNG files are decoded to compute `rgba-v1` pixel identities without re-encoding disk files.
* **Error Resilience**: If an individual stored file is missing or corrupt, the migration logs a warning, skips the damaged entry, and continues. Migration failures do not terminate the daemon.

### 4. Signal Handling & Shutdown
* **`SIGINT` / `SIGTERM`**: Breaks the main event loop and executes clean teardown in strict sequence:
  1. **Abort Migration Task**: Calls `migration_task.abort()` and awaits its handle (`let _ = migration_task.await;`). This cancels pending entries in the migration queue. An already-running `spawn_blocking` decode closure cannot be cancelled preemptively and may finish naturally during runtime teardown; uncommitted database transactions safely roll back on drop. Shutdown does not wait for the remaining queue.
  2. **Activation Shutdown**: Calls `activation_service.shutdown()`, terminating active paste backend sessions (e.g. EIS remote desktop).
  3. **Drop Clipboard Watcher**: Clipboard watcher session is explicitly dropped (`drop(_clipboard_watcher_session)`); remaining owned listener/worker resources are released during normal scope teardown as `main()` returns.
  4. **Exit**: The daemon logs `"Pookie daemon stopped"` and returns cleanly.
* **`SIGHUP`**: Triggers a configuration reload via [`ReloadCoordinator`](../../crates/daemon/src/reload_coordinator.rs). If the configuration file is malformed, the error is logged and existing runtime bindings are preserved.

---

## Ephemeral UI Lifecycle & UiLauncher

The popup UI (`pookie-paste-ui`) is spawned on demand when triggered by a global shortcut or CLI toggle:

### Single-Instance Management (`UiLauncher`)
The daemon regulates UI processes using [`UiLauncher`](../../crates/daemon/src/ui_launcher.rs):
1. **Atomic Launch Gate**: An `AtomicBool` flag (`popup_running`) guards process creation using `compare_exchange(false, true)`.
2. **Process Spawn**: The daemon locates `pookie-paste-ui` beside its own executable (or within standard search paths) and spawns it as a child process.
3. **Reaper Thread**: A background thread waits for child process termination (`child.wait()`), reaps the process, and resets the atomic flag to `false`.

### Pre-Popup Focus Target Capture
When `pookie-paste-ui` starts up, its very first action—before mapping any window or initializing the `eframe` renderer—is to send an IPC request:

```rust
// crates/ui/src/main.rs
let target_id = capture_initial_focus_target();
```

Because the popup window has not yet been mapped, the user's previously active application still holds window manager focus. The daemon queries the platform focus backend and returns a typed [`IpcFocusTarget`](../../crates/ipc/src/protocol.rs). This target identifier is:
1. Used under X11 to calculate window geometry coordinates and anchor the popup near the active target window or cursor. On Wayland, unprivileged window geometry queries are disallowed, so the UI relies on compositor window placement rules.
2. Passed back to the daemon during `ActivateItem` so the daemon knows exactly which window must regain focus before paste injection. If target capture returns `None` (or was omitted), the activation service halts before paste capability evaluation and returns `ClipboardUpdated` without attempting synthetic paste.

### Dismissal & Termination
The UI process terminates upon:
* **Item Activation**: A history item is clicked or selected with Enter. The popup hides while the daemon restores focus and pastes; it closes permanently upon a successful outcome (`Pasted` or `ClipboardUpdated`), or unhides with an error message if paste restoration fails.
* **Explicit Cancellation**: The user presses Escape (canceling active sub-states such as context menus or shortcut recording first), or clicks the header close button (`✕`).
* **Platform Focus-Loss Policy**: The popup window loses input focus after having initially acquired focus (`has_received_focus && !viewport_focused && !activation_in_progress`), evaluated against the startup `FocusLossDismissalPolicy`:
  * **X11 and KDE Plasma Wayland (`Dismiss`)**: Pookie preserves focus-loss dismissal on X11 and KDE Plasma Wayland. When focus is transferred away from the popup, the popup closes.
  * **Sway and Hyprland (`Ignore`)**: On Sway and Hyprland, bare focus loss is ignored because compositor-driven focus changes can occur without dismissal intent. Pointer movement into or out of the popup does not close it, and clicking outside does not dismiss it. Explicit dismissal remains available through Escape, the header close button (`✕`), or item activation.
  * **Unknown / Generic Wayland (`Dismiss`)**: Falls back to dismissal on focus loss to preserve standard desktop popup expectations.

---

## Environment & Platform Resolution

During daemon startup, [`EnvironmentAudit::detect()`](../../crates/daemon/src/platform/environment.rs) parses standard environment variables into structured categories:

```text
EnvironmentAudit
├── session: SessionKind (X11 | Wayland | Unknown)
└── desktop: DesktopKind (Kde | Gnome | Wlroots | Other)
```

Capability resolvers in [`crates/daemon/src/platform/resolvers.rs`](../../crates/daemon/src/platform/resolvers.rs) map these audits to platform-specific implementations:

| Subsystem | X11 | KDE Plasma Wayland | Sway (wlroots) | Hyprland |
| --- | --- | --- | --- | --- |
| **Clipboard** | `X11Clipboard` (`arboard`) | `WaylandClipboard` (`wl-clipboard-rs`) | `WaylandClipboard` (`wl-clipboard-rs`) | `WaylandClipboard` (`wl-clipboard-rs`) |
| **Watcher** | `X11ClipboardWatcher` (XFixes event-driven / fallback polling) | `WaylandClipboardWatcher` (`ext`/`wlr`) | `WaylandClipboardWatcher` (`ext`/`wlr`) | `WaylandClipboardWatcher` (`ext`/`wlr`) |
| **Focus** | `X11FocusBackend` (`_NET_ACTIVE_WINDOW`) | `KdeFocusBackend` (KWin D-Bus helper) | `SwayFocusBackend` (`$SWAYSOCK` IPC) | `HyprlandFocusBackend` (Hyprland IPC) |
| **Paste** | `X11PasteBackend` (XTest fake input) | `PortalEisPasteBackend` (Portal RemoteDesktop + EIS) | `WlrootsPasteBackend` (`zwp_virtual_keyboard_v1`) | `WlrootsPasteBackend` (`zwp_virtual_keyboard_v1`) |
| **Shortcuts** | `X11ShortcutBackend` (passive root grabs) | `WaylandShortcutBackend` (Portal GlobalShortcuts) | `SwayShortcutBackend` (compositor-managed) | `HyprlandShortcutBackend` (compositor-managed) |

If an environment lacks focus restoration support, the daemon resolves to `UnavailableFocusBackend` and forces paste capability to `WaylandPasteBackend` (`PasteCapability::ClipboardOnly`), ensuring safe fallback operation without input injection risk.

---

## Filesystem & Runtime Path Layout

Pookie Paste strictly adheres to the XDG Base Directory Specification:

| Path Category | Environment Variable / Primary Path | Default Fallback | Purpose |
| --- | --- | --- | --- |
| **Data Directory** | `$XDG_DATA_HOME` | `~/.local/share/pookie-paste/` | Contains `pookie-paste.db` and the `images/` directory. |
| **Image Store** | `$XDG_DATA_HOME` | `~/.local/share/pookie-paste/images/` | Standalone canonical PNG files referenced by UUID. |
| **Config Directory** | `$XDG_CONFIG_HOME` | `~/.config/pookie-paste/` | Contains user settings in `config.toml` (also checks legacy/convenience path `~/.config/pookie/config.toml`). |
| **State Directory** | `$XDG_STATE_HOME` | `~/.local/state/pookie-paste/` | Stores runtime state tokens such as `remote-desktop.restore-token`. |
| **Runtime Socket** | `$XDG_RUNTIME_DIR/pookie-paste/pookie.sock` | `/tmp/pookie-paste-<EUID>/pookie.sock` | Local Unix domain stream socket for daemon/UI/CLI communication. |

---

## Related Documentation

* [**Architecture Overview**](overview.md): High-level system structure, architectural boundaries, and crate organization.
* [**Clipboard Subsystem**](../clipboard/overview.md): Clipboard capture, MIME negotiation, and normalization.
* [**Activation Subsystem**](../activation/overview.md): Focus restoration, target confirmation, and paste injection.
* [**Global Shortcuts**](../shortcuts/overview.md): Keybinding architecture, rebind transactions, and status model.
* [**IPC Subsystem**](../ipc/overview.md): Unix domain socket transport, framing codec, and request dispatch.

---

## Implementation References

| Component | Responsibility | Repository File Path |
| --- | --- | --- |
| **Main Daemon Entry** | Daemon initialization, service wiring, and signal loop | [`crates/daemon/src/main.rs`](../../crates/daemon/src/main.rs) |
| **CLI Dispatcher** | Argument parsing and IPC client command handling | [`crates/daemon/src/cli.rs`](../../crates/daemon/src/cli.rs) |
| **UI Launcher** | Atomic single-instance UI spawning and process reaping | [`crates/daemon/src/ui_launcher.rs`](../../crates/daemon/src/ui_launcher.rs) |
| **Environment Audit** | Desktop session detection from environment variables | [`crates/daemon/src/platform/environment.rs`](../../crates/daemon/src/platform/environment.rs) |
| **Backend Resolvers** | Capability-based platform backend instantiators | [`crates/daemon/src/platform/resolvers.rs`](../../crates/daemon/src/platform/resolvers.rs) |
| **Application Paths** | XDG path discovery and fallback resolution | [`crates/daemon/src/app_paths.rs`](../../crates/daemon/src/app_paths.rs) |
| **IPC Server** | Socket binding, stale socket cleanup, and client loop | [`crates/daemon/src/ipc_server.rs`](../../crates/daemon/src/ipc_server.rs) |
| **UI Entry Point** | Pre-popup focus capture and window creation | [`crates/ui/src/main.rs`](../../crates/ui/src/main.rs) |
