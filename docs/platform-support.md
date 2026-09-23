# Platform Support Matrix & Architecture Guide

Pookie Paste is designed with a platform-aware, modular architecture. It decouples clipboard capture, focus management, and paste injection into distinct abstraction layers. This allows the core history and UI systems to remain platform-agnostic while native backends handle the unique requirements of X11 and diverse Wayland compositors.

This document details all supported environments, their backend components, and architectural considerations as of Phase 13.

---

## Supported Platforms Matrix

| Platform / Desktop | Session Type | Focus Backend | Paste Backend | Direct Paste | Verification Status |
| :--- | :--- | :--- | :--- | :---: | :--- |
| **X11** | X11 | `X11FocusBackend` | `X11PasteBackend` | **Yes** | Verified (Linux Mint / X11) |
| **KDE Plasma** | Wayland | `KdeFocusBackend` | `PortalEisPasteBackend` | **Yes** | Verified (KDE Plasma 6 Wayland) |
| **Sway** | Wayland | `SwayFocusBackend` | `WlrootsPasteBackend` | **Yes** | Verified (Sway 1.9+ / wlroots) |
| **Hyprland** | Wayland | `HyprlandFocusBackend` | `WlrootsPasteBackend` | **Yes** | Verified (Hyprland 0.56+) |
| **Generic / Unknown** | Wayland | `UnavailableFocusBackend` | `WaylandPasteBackend` | **No** *(Clipboard only)* | Fallback / Safe Mode |

---

## Backend Details

### 1. X11

X11 uses a shared, cooperative windowing architecture where clients can inspect global window properties and inject synthetic input events.

#### Focus Management (`X11FocusBackend`)
- **Capture**: Queries the X11 root window property `_NET_ACTIVE_WINDOW` using `x11rb` to obtain the currently active window `u32` ID (`FocusTarget::X11(u32)`).
- **Validation**: Verifies that the window exists and is managed by an EWMH-compliant window manager.
- **Restoration**: Dispatches a `_NET_ACTIVE_WINDOW` `ClientMessage` to the root window with a source indication of `2` (direct application request), prompting the window manager to restore focus.

#### Paste Injection (`X11PasteBackend`)
- **Injection**: Uses the `XTest` extension via `x11rb::protocol::xtest::fake_input` to synthesize keyboard events for Left Control (`KEY_LEFTCTRL = 29`) and `V` (`KEY_V = 47`).
- **Synchronization**: Emits press and release events separated by a short interval and flushes the X11 connection.

#### Why It Works
X11 grants clients global window inspection and synthetic event delivery via standard protocols (`EWMH` and `XTest`), requiring no compositor-specific IPC.

---

### 2. KDE Plasma Wayland

KDE Plasma implements strict Wayland security boundaries and does not expose unprivileged virtual keyboard protocols. Instead, it relies on KWin scripting and the XDG Desktop Portal ecosystem.

#### Focus Management (`KdeFocusBackend`)
- **Capture & Restoration**: Communicates via D-Bus with a dedicated KWin helper script that tracks and manages KWin window instances using persistent UUIDs (`FocusTarget::Kde(String)`).
- **Verification**: `FocusService` verifies that KWin has successfully switched the active window to the target UUID before direct paste injection begins.

#### Paste Injection (`PortalEisPasteBackend`)
- **Injection**: Leverages the XDG Desktop Portal `org.freedesktop.portal.RemoteDesktop` interface in conjunction with `libeis` (`ashpd` and `reis` crates).
- **Protocol**: Establishes an Emulated Input Server (EIS) session to inject synthetic `Ctrl+V` input events.
- **Note**: KDE Plasma deliberately does **not** expose `zwp_virtual_keyboard_v1`. Direct paste on KDE Wayland exclusively uses Portal / EIS.

---

### 3. Sway (wlroots)

Sway is a tiling Wayland compositor built on top of `wlroots`. It provides a dedicated UNIX domain socket for window control and supports the standard `wlroots` virtual keyboard protocol.

#### Focus Management (`SwayFocusBackend`)
- **Discovery**: Discovers the Sway IPC socket via the `$SWAYSOCK` environment variable.
- **Capture**: Sends a `GET_TREE` (type `4`) command over the IPC socket, walks the node tree to find the currently focused container, and captures its container ID (`con_id` -> `FocusTarget::Sway(i64)`).
- **Restoration**: Dispatches a `RUN_COMMAND` (type `0`) message with payload `[con_id=<con_id>] focus`.
- **Verification**: Confirms that Sway reports the target container as active.

#### Paste Injection (`WlrootsPasteBackend`)
- **Protocol**: Binds to `zwp_virtual_keyboard_v1` on the active `wl_seat` via Wayland client globals.
- **Keymap**: Generates a standard XKB keymap in a sealed memory file descriptor (`memfd_create`) and uploads it to the compositor.
- **Modifier Synchronization**: Explicitly synchronizes the XKB modifier mask (`MOD_CONTROL = 1 << 2`) via `virtual_keyboard.modifiers()` and uses monotonic timestamps (`CLOCK_MONOTONIC`).

---

### 4. Hyprland (wlroots-compatible)

Hyprland is an independent Wayland compositor that supports `wlroots` input protocols while using a custom UNIX socket IPC protocol for window management.

#### Focus Management (`HyprlandFocusBackend`)
- **Discovery**: Locates the Hyprland command socket at `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`.
- **Capture**: Sends `j/activewindow` to the command socket, parses the JSON response, and extracts the hexadecimal window memory address (`0x...` -> `FocusTarget::Hyprland(String)`).
- **Restoration**: Sends a focus command to the Hyprland dispatcher.
  - *Hyprland 0.56+ (Modern Lua Dispatcher)*: Formats the command as:
    ```text
    dispatch hl.dsp.focus({ window = "address:<address>" })
    ```
  - *Legacy Hyprland*: Earlier versions used `dispatch focuswindow address:<address>`.
- **Verification**: Re-queries `j/activewindow` over IPC to guarantee the window has regained active focus before initiating paste injection.

#### Paste Injection (`WlrootsPasteBackend`)
- Like Sway, Hyprland exposes the Wayland `zwp_virtual_keyboard_v1` protocol.
- Pasting uses `WlrootsPasteBackend` to emit:
  1. `KEY_LEFTCTRL` press + `modifiers(MOD_CONTROL, 0, 0, 0)`
  2. `KEY_V` press
  3. `KEY_V` release
  4. `KEY_LEFTCTRL` release + `modifiers(0, 0, 0, 0)`
  with intervening connection flushes and monotonic millisecond timestamps.

---

## Wayland Architecture Notes

### Why Multiple Wayland Backends Exist

Unlike X11, Wayland is not a single monolith; there is **no single cross-desktop protocol** for global window management, foreign window focus, or arbitrary synthetic input injection. Security boundaries prevent arbitrary Wayland clients from focusing other windows or spoofing keystrokes.

As a result:
1. **Window Management**:
   - Sway uses i3/Sway IPC (`$SWAYSOCK`).
   - Hyprland uses Hyprland IPC (`$HYPRLAND_INSTANCE_SIGNATURE`).
   - KDE Plasma uses KWin D-Bus scripting.
   - GNOME restricts window manipulation to Mutter/Shell extensions.
2. **Synthetic Input**:
   - `wlroots`-based compositors (Sway, Hyprland, Wayfire) implement `zwp_virtual_keyboard_v1`.
   - Desktop-agnostic portals use `XDG Desktop Portal RemoteDesktop` + `EIS`.
   - KDE Plasma implements Portal/EIS but avoids `zwp_virtual_keyboard_v1`.

Pookie Paste respects these architectural realities by isolating each compositor's mechanism behind uniform `FocusBackend` and `PasteBackend` traits.

---

## Fallback Behavior (Safe Mode)

If a desktop environment or compositor lacks focus restoration or input simulation support, Pookie Paste gracefully degrades to **Clipboard-Only Fallback**:

```text
Focus restoration check
          │
    ┌─────┴─────┐
    ▼           ▼
Available   Unavailable
    │           │
    ▼           ▼
Direct Paste   PlatformPasteBackend::WaylandFallback
(Ctrl+V)       (PasteCapability::ClipboardOnly)
```

### Safety Invariants:
1. **Never Inject Into an Arbitrary Window**: If target focus cannot be captured or confirmed upon restoration, direct paste is strictly disabled.
2. **Daemon Remains Active**: The daemon continues capturing clipboard history, storing metadata/images, and serving search requests over IPC.
3. **Transparent User Experience**: When an item is selected, Pookie Paste copies the item to the system clipboard, and the user simply uses standard manual paste (`Ctrl+V` or middle-click).

---

## Current Limitations & Future Work

The following items reflect current architectural boundaries and planned enhancements:

### 1. Global Shortcut Management
- **Current State**: Global hotkeys (such as `Super+V`) are bound in the compositor or desktop environment configuration:
  - Hyprland: `bind = $mainMod, V, exec, pookie-paste-ui` in `hyprland.conf`
  - Sway: `bindsym $mod+v exec pookie-paste-ui` in `sway/config`
  - KDE: Custom Shortcuts via System Settings / KGlobalAccel
- **Future Direction**: Implementation of an integrated shortcut manager utilizing the XDG Desktop Portal GlobalShortcuts interface (`org.freedesktop.portal.GlobalShortcuts`) for desktop-independent hotkey registration.

### 2. Wayland Layer Shell Popup (`wlr-layer-shell`)
- **Current State**: The popup UI (`pookie-paste-ui`) is built on `eframe` / `winit` as a standard `xdg_toplevel` undecorated floating window. In tiling compositors like Hyprland, window rules (`windowrulev2 = float, class:^(pookie-paste-ui)$`) are required, and mouse movement can influence focus if `follow_mouse = 1` is configured.
- **Future Direction**: Investigating a native `wlr-layer-shell-v1` overlay surface for wlroots environments (Sway, Hyprland), ensuring the popup opens in the `overlay` layer with dedicated keyboard grab semantics.

---

## Implementation References

| Component | Responsibility | Repository File Path |
| :--- | :--- | :--- |
| **Focus Abstraction** | Core `FocusBackend` trait and `FocusTarget` model | [`crates/daemon/src/focus_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/focus_backend.rs) |
| **Platform Focus Backend** | Enum uniting all focus implementations | [`crates/daemon/src/platform_focus_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/platform_focus_backend.rs) |
| **Paste Abstraction** | Core `PasteBackend` trait and `PlatformPasteBackend` | [`crates/daemon/src/paste_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/paste_backend.rs) |
| **Platform Resolvers** | Capability-based backend resolver routines | [`crates/daemon/src/platform/resolvers.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/platform/resolvers.rs) |
| **Environment Audit** | Session and desktop detection logic | [`crates/daemon/src/platform/environment.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/platform/environment.rs) |
| **X11 Focus** | X11 `_NET_ACTIVE_WINDOW` capture & restore | [`crates/daemon/src/x11_focus_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/x11_focus_backend.rs) |
| **X11 Paste** | X11 `XTest` synthetic keystroke injection | [`crates/daemon/src/x11_paste_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/x11_paste_backend.rs) |
| **KDE Focus** | KWin D-Bus helper focus backend | [`crates/daemon/src/kde_focus_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/kde_focus_backend.rs) |
| **KDE Paste** | Portal RemoteDesktop + `libeis` paste backend | [`crates/daemon/src/portal_eis_paste_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/portal_eis_paste_backend.rs) |
| **Sway Focus** | Sway IPC socket container capture & restore | [`crates/daemon/src/sway_focus_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/sway_focus_backend.rs) |
| **Hyprland Focus** | Hyprland IPC command socket window capture & restore | [`crates/daemon/src/hyprland_focus_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/hyprland_focus_backend.rs) |
| **wlroots Paste** | `zwp_virtual_keyboard_v1` injection for Sway & Hyprland | [`crates/daemon/src/wlroots_paste_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/wlroots_paste_backend.rs) |
| **Wayland Fallback** | Safe clipboard-only paste backend | [`crates/daemon/src/paste_backend.rs`](file:///home/riyan/Riyan/projects/pookie-paste/crates/daemon/src/paste_backend.rs) |
