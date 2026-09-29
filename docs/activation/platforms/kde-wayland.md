# KDE Wayland Activation & Paste

This document details the KDE Plasma (Wayland) platform implementation of window focus restoration and synthetic paste injection in Pookie Paste.

For the common activation lifecycle, see [Activation Overview](../overview.md).

---

## Architecture Overview

Wayland compositors do not expose global window control or arbitrary input injection over unprivileged Wayland protocols. On KDE Plasma, Pookie divides responsibilities across two specialized subsystem interfaces:

1. **Window Focus Management**: Uses an installed KWin scripting helper communicating with the Pookie daemon over session D-Bus.
2. **Synthetic Paste Injection**: Uses the XDG Desktop Portal `RemoteDesktop` interface coupled with the Emulated Input System (EIS / `libei`).

---

## Target Identity

In KDE Plasma on Wayland, window targets are identified by KWin's internal window UUID:

```rust
FocusTarget::Kde(uuid::Uuid)
```

In IPC messages transferred between the UI and daemon, this is serialized as a UUID string:

```rust
IpcFocusTarget::Kde(String)
```

---

## Focus Management ([`crates/daemon/src/kde_focus_backend.rs`](../../../crates/daemon/src/kde_focus_backend.rs))

KDE Plasma does not allow unprivileged client applications to query or manipulate active windows. Pookie bridges this boundary via a small KWin scripting helper.

### 1. D-Bus Interface & Worker Architecture
During initialization, the daemon spawns a dedicated worker thread with its own session D-Bus connection:
* **Service Name**: `io.github.riyanj220.PookiePaste`
* **Object Path**: `/io/github/riyanj220/PookiePaste/Focus`
* **Interface**: `io.github.riyanj220.PookiePaste.Focus`

The daemon exposes callback methods on this object (`Captured`, `CaptureUnavailable`, `RestoreRequested`, `RestoreNotFound`, `Active`, `ActiveNone`).

### 2. KWin Helper Actions
The KWin helper script registers three global shortcut actions with KWin's `kglobalaccel` service (`org.kde.kglobalaccel`, `/component/kwin`, `org.kde.kglobalaccel.Component`):
* **`PookiePasteFocusCapture`**: Inspects `workspace.activeWindow`, obtains its internal UUID, and calls Pookie's `Captured(uuid)` D-Bus method.
* **`PookiePasteFocusRestore`**: Directs KWin to activate the window matching the specified UUID, then calls Pookie's `RestoreRequested(uuid)` D-Bus method.
* **`PookiePasteFocusActive`**: Inspects the current `workspace.activeWindow` and calls Pookie's `Active(uuid)` D-Bus method.

### 3. Capture, Restore, and Confirmation Sequence
1. **Capture**: The daemon calls `kglobalaccel.invokeShortcut("PookiePasteFocusCapture")` and awaits the `Captured` callback, saving the returned UUID as `FocusTarget::Kde`.
2. **Restore**: The daemon calls `kglobalaccel.invokeShortcut("PookiePasteFocusRestore")` and awaits the `RestoreRequested` callback.
3. **Confirmation**: The `FocusService` polling loop repeatedly invokes `is_active(target)`. This triggers `PookiePasteFocusActive` and verifies that the reported active UUID matches the expected target.

---

## Paste Injection via Portal & EIS ([`crates/daemon/src/portal_eis_paste_backend.rs`](../../../crates/daemon/src/portal_eis_paste_backend.rs))

Synthetic keystrokes are injected via the Freedesktop RemoteDesktop portal and the Emulated Input System (EIS):

### 1. Portal Session & EIS Connection
1. Pookie establishes a session with `org.freedesktop.portal.RemoteDesktop` using the `ashpd` crate, requesting `DeviceType::Keyboard`.
2. The portal provides a connected Unix stream socket file descriptor via `ConnectToEIS`.
3. Pookie connects to the socket using the `reis` crate and discovers an emulated keyboard device (`DeviceCapability::Keyboard`).
4. The emulation session remains active across multiple paste operations rather than being re-created for each activation.

### 2. Restore Token & Permission Persistence
To minimize repeated permission prompts, Pookie saves the portal session's restore token:
* **Storage Location**: `$XDG_STATE_HOME/pookie-paste/remote-desktop.restore-token` (falling back to `~/.local/state/pookie-paste/...`).
* **Atomic Writes**: Tokens are written to a temporary sibling file (`.remote-desktop.restore-token.tmp`) before being atomically renamed into place.
* **Portal Authority**: When starting up, Pookie supplies the saved restore token to `SelectDevices` so the portal can restore a previous permission grant or session where supported. The desktop portal remains authoritative and may still prompt the user or require interaction depending on compositor policy and permission state.

### 3. Keystroke Synthesis
Synthetic Ctrl+V is dispatched through the EIS keyboard interface using Linux evdev scancodes:
1. `KEY_LEFTCTRL` (`29`) Press
2. 30ms interval (`KEY_INTERVAL`)
3. `KEY_V` (`47`) Press
4. `KEY_V` (`47`) Release
5. 30ms interval
6. `KEY_LEFTCTRL` (`29`) Release

### 4. Health Tracking & Recovery
The backend tracks runtime health via an atomic state (`Ready`, `Paused`, `Recovering`, `Failed`). If the portal or EIS connection drops:
* The backend transitions to `Recovering` and initiates an automatic reconnection loop with backoff delays (`[1s, 2s, 5s, 10s]`).
* While in `Recovering`, `Paused`, or `Failed` states, `capability()` reports `PasteCapability::ClipboardOnly`. Activations during this period complete clipboard writeback and history promotion without attempting synthetic paste or failing.
* Once reconnection succeeds and the virtual keyboard device resumes, health transitions back to `Ready` (`PasteCapability::Direct`).

---

## Implementation References

| Component | Responsibility | Repository File Path |
| --- | --- | --- |
| **KDE Focus Backend** | KWin D-Bus service, `kglobalaccel` invocation, and callback handling | [`crates/daemon/src/kde_focus_backend.rs`](../../../crates/daemon/src/kde_focus_backend.rs) |
| **Portal / EIS Backend** | RemoteDesktop session, restore token management, and EIS input synthesis | [`crates/daemon/src/portal_eis_paste_backend.rs`](../../../crates/daemon/src/portal_eis_paste_backend.rs) |
| **Common Focus Service** | 5ms/250ms polling loop driving focus confirmation | [`crates/daemon/src/focus_service.rs`](../../../crates/daemon/src/focus_service.rs) |
