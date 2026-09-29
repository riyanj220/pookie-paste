# Hyprland Activation & Paste

This document details the Hyprland platform implementation of window focus restoration and synthetic paste injection in Pookie Paste.

For the common activation lifecycle, see [Activation Overview](../overview.md).

---

## Target Identity

In Hyprland, window targets are represented by their unique hexadecimal window memory address:

```rust
FocusTarget::Hyprland(String)
```

In IPC messages transferred between the UI and daemon, this corresponds to `IpcFocusTarget::Hyprland(String)` (for example, `"0x55a72f1b8a90"`).

---

## Focus Management ([`crates/daemon/src/hyprland_focus_backend.rs`](../../../crates/daemon/src/hyprland_focus_backend.rs))

Hyprland focus operations communicate over Hyprland's Unix domain command socket.

### 1. IPC Socket Discovery
The backend locates the compositor command socket (`.socket.sock`) using the environment signature variable:
```text
$HYPRLAND_INSTANCE_SIGNATURE
```

Pookie inspects candidate socket paths in order:
1. `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`
2. `/tmp/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`

### 2. Active Window Capture
1. Sends the JSON query command `j/activewindow` over the IPC stream.
2. Parses the JSON response object to extract the `address` field.
3. Records the hex string as `FocusTarget::Hyprland(address)`.

### 3. Focus Restoration Request
1. Constructs a Hyprland dispatcher focus command targeting the window address:
   ```text
   dispatch hl.dsp.focus({ window = "address:<address>" })
   ```
2. Sends the command over the IPC socket.
3. Verifies that Hyprland responds with `"ok"`.

### 4. Confirmation
During the focus confirmation loop, `is_active(target)` sends `j/activewindow` to Hyprland and compares the returned `address` against the target address using case-insensitive string matching (`eq_ignore_ascii_case`).

---

## Paste Injection ([`crates/daemon/src/wlroots_paste_backend.rs`](../../../crates/daemon/src/wlroots_paste_backend.rs))

Hyprland shares the same `WlrootsPasteBackend` used by Sway:
* **Protocol**: Binds the `zwp_virtual_keyboard_v1` protocol on the active Wayland seat.
* **Mechanism**: Creates a virtual keyboard instance with an anonymous shared memory file (`memfd`) containing an XKB keymap (mapping Linux evdev scancodes `29` to Left Control and `47` to 'V').
* **Timing**: Synthesizes Ctrl+V key press and release events separated by calibrated 25ms pauses.

For full protocol and keymap details, see [Sway Activation & Paste](sway.md).

---

## Implementation References

| Component | Responsibility | Repository File Path |
| --- | --- | --- |
| **Hyprland Focus Backend** | IPC socket discovery, `j/activewindow` queries, and dispatcher focus restoration | [`crates/daemon/src/hyprland_focus_backend.rs`](../../../crates/daemon/src/hyprland_focus_backend.rs) |
| **Wlroots Paste Backend** | `zwp_virtual_keyboard_v1` binding and virtual keystroke injection | [`crates/daemon/src/wlroots_paste_backend.rs`](../../../crates/daemon/src/wlroots_paste_backend.rs) |
| **Common Focus Service** | 5ms/250ms polling loop driving focus confirmation | [`crates/daemon/src/focus_service.rs`](../../../crates/daemon/src/focus_service.rs) |
