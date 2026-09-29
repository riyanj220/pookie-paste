# Sway Activation & Paste

This document details the Sway platform implementation of window focus restoration and synthetic paste injection in Pookie Paste.

For the common activation lifecycle, see [Activation Overview](../overview.md).

---

## Target Identity

In Sway, window targets are represented by their Sway container identifier (`con_id`):

```rust
FocusTarget::Sway(i64)
```

In IPC messages transferred between the UI and daemon, this corresponds to `IpcFocusTarget::Sway(i64)`.

---

## Focus Management ([`crates/daemon/src/sway_focus_backend.rs`](../../../crates/daemon/src/sway_focus_backend.rs))

Sway focus operations communicate over Sway's Unix domain socket using the i3/Sway IPC protocol.

### 1. IPC Socket Discovery
The backend discovers the active compositor socket path through the standard environment variable:
```text
$SWAYSOCK
```

All IPC messages use the standard framing: magic bytes `i3-ipc`, payload length (32-bit integer), message type (32-bit integer), and payload.

### 2. Active Container Capture
1. Sends IPC message type `GET_TREE` (`4`).
2. Recursively parses the returned JSON layout tree to locate the container where `"focused": true`.
3. Extracts the container's numeric `id` and records it as `FocusTarget::Sway(id)`.

### 3. Focus Restoration Request
1. Constructs an IPC focus command using container criteria:
   ```text
   [con_id=<target_id>] focus
   ```
2. Sends the command via IPC message type `RUN_COMMAND` (`0`).
3. Parses the JSON array response and verifies that the operation returned `{"success": true}`.

### 4. Confirmation
During the focus confirmation loop, `is_active(target)` re-queries `GET_TREE` and parses the current layout tree. Focus is confirmed when the newly focused container matches the expected `con_id`.

---

## Paste Injection via Virtual Keyboard ([`crates/daemon/src/wlroots_paste_backend.rs`](../../../crates/daemon/src/wlroots_paste_backend.rs))

Synthetic paste injection on Sway uses the wlroots virtual keyboard protocol (`zwp_virtual_keyboard_v1`), shared with Hyprland:

### 1. Protocol Binding & Keymap Setup
1. The daemon establishes a Wayland client connection and binds `wl_seat` and `zwp_virtual_keyboard_manager_v1`.
2. Creates a virtual keyboard instance tied to the active seat.
3. Allocates an anonymous shared memory file (`memfd`) containing an XKB keymap string:
   * Maps Linux evdev scancode `29` to `<LCTL>` / `Control_L`.
   * Maps Linux evdev scancode `47` to `<AB04>` / `v` / `V`.
4. Passes the file descriptor to `virtual_keyboard.keymap()`.

### 2. Keystroke Synthesis
Synthetic Ctrl+V is dispatched with calibrated 25ms intervals (`KEY_INTERVAL`) to ensure the target application processes state transitions:
1. **Press Left Control**: Sends `KEY_LEFTCTRL` (`29`) key event with `MOD_CONTROL` modifier mask and flushes the connection.
2. **Press V**: Waits 25ms, sends `KEY_V` (`47`) key press event, and flushes.
3. **Release V**: Waits 25ms, sends `KEY_V` (`47`) key release event, and flushes.
4. **Release Left Control**: Waits 25ms, sends `KEY_LEFTCTRL` (`29`) key release event, clears all modifiers, and flushes.

---

## Implementation References

| Component | Responsibility | Repository File Path |
| --- | --- | --- |
| **Sway Focus Backend** | IPC socket communication, `GET_TREE` parsing, and `RUN_COMMAND` focus requests | [`crates/daemon/src/sway_focus_backend.rs`](../../../crates/daemon/src/sway_focus_backend.rs) |
| **Wlroots Paste Backend** | `zwp_virtual_keyboard_v1` binding and virtual keystroke injection | [`crates/daemon/src/wlroots_paste_backend.rs`](../../../crates/daemon/src/wlroots_paste_backend.rs) |
| **Common Focus Service** | 5ms/250ms polling loop driving focus confirmation | [`crates/daemon/src/focus_service.rs`](../../../crates/daemon/src/focus_service.rs) |
