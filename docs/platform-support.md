# Platform Support Matrix & Architecture Guide

Pookie Paste employs a platform-aware, modular architecture. It decouples clipboard monitoring, focus restoration, paste injection, and global shortcuts into distinct abstraction layers. This allows the core history and UI systems to remain platform-agnostic while native backends handle the unique requirements of X11 and diverse Wayland compositors.

For comprehensive implementation details, see the modular guides in [Platform Documentation](#platform-documentation).

---

## Platform Support Matrix

| Platform / Desktop | Session Type | Clipboard Support | Focus Restoration | Paste Injection | Shortcut Mechanism | Support Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **X11** | X11 | `X11Clipboard` (polling watcher) | `X11FocusBackend` (`_NET_ACTIVE_WINDOW`) | `X11PasteBackend` (XTest fake input) | `Native` (passive root-window key grabs) | **Verified** |
| **KDE Plasma** | Wayland | `WaylandClipboard` (`ext`/`wlr` data-control) | `KdeFocusBackend` (KWin D-Bus helper) | `PortalEisPasteBackend` (RemoteDesktop + EIS) | `Portal` (XDG GlobalShortcuts D-Bus) | **Verified** (tested on Plasma 6) |
| **Sway** | Wayland | `WaylandClipboard` (`ext`/`wlr` data-control) | `SwayFocusBackend` (`$SWAYSOCK` IPC) | `WlrootsPasteBackend` (`zwp_virtual_keyboard_v1`) | `CompositorManaged` (Sway `bindsym` + IPC inspection) | **Verified** (tested on Sway 1.9) |
| **Hyprland** | Wayland | `WaylandClipboard` (`ext`/`wlr` data-control) | `HyprlandFocusBackend` (Hyprland IPC) | `WlrootsPasteBackend` (`zwp_virtual_keyboard_v1`) | `CompositorManaged` (Hyprland `bind` + IPC inspection) | **Verified** (tested on Hyprland 0.56+) |
| **Generic / Unknown** | Wayland | `WaylandClipboard` (`ext`/`wlr` if available) | `UnavailableFocusBackend` | `WaylandPasteBackend` (Clipboard-only) | `Unsupported` (CLI invocation only) | **Fallback / Safe Mode** |

---

## Desktop Environment Architecture

### 1. X11
* **Architecture**: Direct display server interaction using `x11rb`.
* **Clipboard**: Continuous monitoring via periodic polling (`POLL_INTERVAL = 500ms`) and `arboard` read/write access.
* **Focus & Paste**: Window capture and restoration via EWMH `_NET_ACTIVE_WINDOW` root-window messages; synthetic Ctrl+V keystroke injection via the `XTest` extension.
* **Popup Lifecycle**: Focus loss is treated as a dismissal signal (`FocusLossDismissalPolicy::Dismiss`); when focus transfers away from the popup, the popup closes.
* **Shortcuts**: Pookie directly owns the global keybinding using transactional passive root-window grabs (`XGrabKey`) covering all `NumLock` and `CapsLock` modifier mask permutations.
* *Details: [X11 Activation](activation/platforms/x11.md) | [X11 Shortcuts](shortcuts/platforms/x11.md)*

### 2. KDE Plasma (Wayland)
* **Architecture**: Interacts across session D-Bus and standard Wayland protocols.
* **Clipboard**: Continuous event-driven monitoring using modern `ext-data-control-v1` (falling back to `zwlr_data_control_v1`) and `wl-clipboard-rs`.
* **Focus & Paste**: Window identity tracking and focus switching via a dedicated KWin D-Bus helper script (`org.kde.kglobalaccel`); synthetic keystroke injection via XDG Desktop Portal `RemoteDesktop` and the Emulated Input System (EIS / `libei`).
* **Popup Lifecycle**: Focus loss is treated as a dismissal signal (`FocusLossDismissalPolicy::Dismiss`); when focus transfers away from the popup, the popup closes.
* **Shortcuts**: Managed through the XDG Desktop Portal `GlobalShortcuts` interface (`org.freedesktop.portal.GlobalShortcuts`). The portal-reported effective shortcut is authoritative over `config.toml`, and graphical rebinding opens the portal's native configuration dialog.
* *Details: [KDE Wayland Activation](activation/platforms/kde-wayland.md) | [KDE Desktop Portal Shortcuts](shortcuts/platforms/kde-portal.md)*

### 3. Sway (wlroots)
* **Architecture**: Communicates with the live compositor over Sway's Unix domain socket (`$SWAYSOCK`).
* **Clipboard**: Event-driven monitoring via `wlr-data-control-v1` / `ext-data-control-v1` protocols.
* **Focus & Paste**: Container capture and focus switching via Sway IPC (`GET_TREE` and `[con_id=...] focus`); synthetic Ctrl+V injection via the `zwp_virtual_keyboard_v1` protocol.
* **Popup Lifecycle**: Bare focus loss is ignored (`FocusLossDismissalPolicy::Ignore`). Pointer motion into or out of the popup does not close it, and clicking outside does not dismiss it. Explicit dismissal remains Escape, the header close button (`✕`), or item activation.
* **Shortcuts**: Compositor-managed. Sway owns the live keybinding and executes `pookie-paste --toggle`. Pookie records desired intent in `~/.config/pookie-paste/config.toml`, formats the canonical `bindsym` snippet, and verifies status authoritatively over live Sway IPC (`GET_CONFIG`). Pookie never silently modifies `~/.config/sway/config`.
* *Details: [Sway Activation](activation/platforms/sway.md) | [Sway Shortcuts](shortcuts/platforms/sway.md)*

### 4. Hyprland
* **Architecture**: Communicates with Hyprland over its Unix domain command socket (`$HYPRLAND_INSTANCE_SIGNATURE`).
* **Clipboard**: Event-driven monitoring via `ext-data-control-v1` / `wlr-data-control-v1`.
* **Focus & Paste**: Hexadecimal window memory address capture (`j/activewindow`) and dispatcher focus restoration; synthetic Ctrl+V injection via `zwp_virtual_keyboard_v1`.
* **Popup Lifecycle**: Bare focus loss is ignored (`FocusLossDismissalPolicy::Ignore`). Pointer motion into or out of the popup does not close it, and clicking outside does not dismiss it. Explicit dismissal remains Escape, the header close button (`✕`), or item activation.
* **Shortcuts**: Compositor-managed. Hyprland captures the shortcut and executes `pookie-paste --toggle`. Pookie inspects live active bindings via `j/binds` IPC. If bound to an opaque Lua callback (`__lua`), Pookie truthfully classifies the binding as `BoundUnverified` (accepted as valid in the UI without false upgrades). Pookie never silently modifies `hyprland.conf` or `hyprland.lua`.
* *Details: [Hyprland Activation](activation/platforms/hyprland.md) | [Hyprland Shortcuts](shortcuts/platforms/hyprland.md)*

---

## Safe Mode & Fallback Behavior

When running in an unsupported environment, when direct paste capability is unavailable, or during activation failures:

1. **Direct Paste Unavailable (`ClipboardOnly`)**: If the platform resolver assigns `PasteCapability::ClipboardOnly` (e.g. generic or unsupported Wayland compositors), activation writes the item to the system clipboard, promotes it in history, and returns `ClipboardUpdated` without attempting synthetic input.
2. **Missing Target (`target = None`)**: If no focus target was captured before opening the popup, activation completes clipboard writeback and history promotion, then safely halts before paste capability evaluation and returns `ClipboardUpdated`.
3. **Focus Confirmation Failure (`PasteFailed`)**: If a target was captured (`target = Some(target)`) but window focus cannot be restored or confirmed within the 250ms polling window, activation aborts paste injection and returns `PasteFailed`.
4. **No Unverified Input**: Across all three cases, synthetic keystrokes are never emitted into an unconfirmed or unverified window.
5. **Daemon Quiescence**: Clipboard history capture, image storage, and search queries remain fully functional. The user can paste manually using the application's normal paste command (for example, Ctrl+V).

---

## Current Known Limitations

1. **Wayland Floating Window Placement & Tiling Lifecycles**:
   The popup UI (`pookie-paste-ui`) is built on `eframe`/`winit` as a standard `xdg_toplevel` window. In tiling compositors (Sway, Hyprland), window rules (`windowrulev2 = float, class:^(pookie-paste-ui)$` or `for_window [app_id="pookie-paste-ui"] floating enable`) are recommended to ensure floating presentation. Additionally, because pointer motion across window boundaries can trigger focus transitions on these compositors, Pookie ignores bare focus loss on Sway and Hyprland, requiring explicit dismissal (Escape, header close button, or item selection).
2. **Wayland Shortcut Recording Constraint**:
   When recording a shortcut in the UI, if the compositor already has that exact key chord bound, the compositor intercepts the key event before the popup window receives it. To rebind an existing key chord, users may update `config.toml` directly or temporarily disable the compositor binding.
3. **Toggle Semantics (`--toggle`)**:
   `pookie-paste --toggle` requests popup display through the daemon while preserving single-instance UI behavior. It does not implement a true visibility toggle (it does not close an already-visible popup window on Wayland).

---

## Platform Documentation

Detailed architecture guides for each subsystem and platform:

* **Subsystem Architecture**:
  * [**Activation & Focus Overview**](activation/overview.md)
  * [**Global Shortcuts Overview**](shortcuts/overview.md)
  * [**Shortcut Status Model**](shortcuts/status-model.md)
  * [**Clipboard Subsystem Overview**](clipboard/overview.md)
  * [**IPC Architecture Overview**](ipc/overview.md)
* **Platform Implementation Guides**:
  * [**X11 Activation & Paste**](activation/platforms/x11.md) | [**X11 Shortcuts**](shortcuts/platforms/x11.md)
  * [**KDE Wayland Activation & Paste**](activation/platforms/kde-wayland.md) | [**KDE Desktop Portal Shortcuts**](shortcuts/platforms/kde-portal.md)
  * [**Sway Activation & Paste**](activation/platforms/sway.md) | [**Sway Shortcuts**](shortcuts/platforms/sway.md)
  * [**Hyprland Activation & Paste**](activation/platforms/hyprland.md) | [**Hyprland Shortcuts**](shortcuts/platforms/hyprland.md)

---

## Implementation References

| Component | Responsibility | Repository File Path |
| :--- | :--- | :--- |
| **Focus Abstraction** | Core `FocusBackend` trait and `FocusTarget` model | [`crates/daemon/src/focus_backend.rs`](../crates/daemon/src/focus_backend.rs) |
| **Paste Abstraction** | Core `PasteBackend` trait and capability enums | [`crates/daemon/src/paste_backend.rs`](../crates/daemon/src/paste_backend.rs) |
| **Shortcut Abstraction** | Core `ShortcutBackend` trait and capability enums | [`crates/daemon/src/shortcut_backend.rs`](../crates/daemon/src/shortcut_backend.rs) |
| **Platform Resolvers** | Capability-based backend resolver routines | [`crates/daemon/src/platform/resolvers.rs`](../crates/daemon/src/platform/resolvers.rs) |
| **Environment Audit** | Session and desktop detection logic | [`crates/daemon/src/platform/environment.rs`](../crates/daemon/src/platform/environment.rs) |
| **X11 Focus** | X11 `_NET_ACTIVE_WINDOW` capture & restore | [`crates/daemon/src/x11_focus_backend.rs`](../crates/daemon/src/x11_focus_backend.rs) |
| **X11 Paste** | X11 `XTest` synthetic keystroke injection | [`crates/daemon/src/x11_paste_backend.rs`](../crates/daemon/src/x11_paste_backend.rs) |
| **X11 Shortcuts** | X11 passive root-window key grabs | [`crates/daemon/src/x11_shortcut_backend.rs`](../crates/daemon/src/x11_shortcut_backend.rs) |
| **KDE Focus** | KWin D-Bus helper focus backend | [`crates/daemon/src/kde_focus_backend.rs`](../crates/daemon/src/kde_focus_backend.rs) |
| **KDE Paste** | Portal RemoteDesktop + `libeis` paste backend | [`crates/daemon/src/portal_eis_paste_backend.rs`](../crates/daemon/src/portal_eis_paste_backend.rs) |
| **KDE Shortcuts** | Desktop Portal GlobalShortcuts client | [`crates/daemon/src/wayland_shortcut_backend.rs`](../crates/daemon/src/wayland_shortcut_backend.rs) |
| **Sway Focus** | Sway IPC socket container capture & restore | [`crates/daemon/src/sway_focus_backend.rs`](../crates/daemon/src/sway_focus_backend.rs) |
| **Sway Shortcuts** | Sway IPC config diagnosis & verification | [`crates/daemon/src/sway_shortcut_backend.rs`](../crates/daemon/src/sway_shortcut_backend.rs) |
| **Hyprland Focus** | Hyprland IPC command socket window capture & restore | [`crates/daemon/src/hyprland_focus_backend.rs`](../crates/daemon/src/hyprland_focus_backend.rs) |
| **Hyprland Shortcuts** | Hyprland IPC `j/binds` inspection & verification | [`crates/daemon/src/hyprland_shortcut_backend.rs`](../crates/daemon/src/hyprland_shortcut_backend.rs) |
| **wlroots Paste** | `zwp_virtual_keyboard_v1` injection for Sway & Hyprland | [`crates/daemon/src/wlroots_paste_backend.rs`](../crates/daemon/src/wlroots_paste_backend.rs) |
| **Wayland Fallback** | Safe clipboard-only paste backend | [`crates/daemon/src/paste_backend.rs`](../crates/daemon/src/paste_backend.rs) |
