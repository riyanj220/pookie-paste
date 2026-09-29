# Shortcut Status Model

This document explains Pookie Paste's shortcut status model: how runtime status is decoupled from persistent configuration, what each status enum variant represents, and how compositor binding states are interpreted by the daemon and UI.

---

## Why Status Is Separate from Configuration

In simple single-user utilities, configuration file content is often assumed to reflect runtime reality. In modern Linux desktop environments, that assumption fails:

1. **Distributed Shortcut Ownership**: On Wayland, the application process does not hold key grabs directly. Global shortcuts are owned either by a desktop portal or by the window manager/compositor.
2. **Asynchronous Drift**: A user can reconfigure their desktop environment (e.g. modifying `~/.config/sway/config` or changing KDE Plasma Shortcuts in System Settings) without Pookie being restarted.
3. **Partial Verification**: In compositors supporting programmable scripting (such as Hyprland with Lua callbacks), a shortcut may be fully operational at runtime even though IPC cannot definitively inspect the internal callback target.

Pookie strictly separates **user intent** (the desired key combination stored in `config.toml`) from **runtime truth** (the live capability, active registration, and compositor binding state reported over IPC via `ShortcutStatusInfo`).

---

## Wire Data Structure: `ShortcutStatusInfo`

Defined in [`crates/ipc/src/protocol.rs`](../../crates/ipc/src/protocol.rs), `ShortcutStatusInfo` is returned by `GetShortcutStatus`, `RecheckShortcutStatus`, and `SetShortcut`:

```rust
pub struct ShortcutStatusInfo {
    pub configured_shortcut: String,
    pub backend_name: Option<String>,
    pub capability: Option<IpcShortcutCapability>,
    pub effective_shortcut: Option<String>,
    pub state: IpcShortcutState,
}
```

* **`configured_shortcut`**: The string representation of the primary shortcut from `config.toml` (e.g. `"Super+V"`).
* **`backend_name`**: Human-readable name of the active platform backend (e.g. `"X11 global shortcut"`, `"Hyprland compositor-managed shortcut"`).
* **`capability`**: The mechanism used by the platform to manage shortcuts.
* **`effective_shortcut`**: The trigger string actually active on the platform. On KDE Plasma, this contains the portal-reported assignment (which may differ from `configured_shortcut`). On X11, this matches `configured_shortcut`. On compositor-managed backends, this is `None`.
* **`state`**: Rich algebraic state describing active health, compositor bindings, and diagnostics.

---

## Capability Model: `IpcShortcutCapability`

```rust
pub enum IpcShortcutCapability {
    Native,
    Portal,
    CompositorManaged,
    Unsupported,
}
```

* **`Native`**: The daemon directly registers passive key grabs with the display server (X11 `XGrabKey`).
* **`Portal`**: Global shortcuts are managed through the XDG Desktop Portal (`org.freedesktop.portal.GlobalShortcuts`) over D-Bus.
* **`CompositorManaged`**: Keybindings are configured directly in the window manager or compositor, which invokes `pookie-paste --toggle` over daemon IPC (Sway, Hyprland).
* **`Unsupported`**: Global shortcuts cannot be used in the current session (e.g. unsupported Wayland compositors lacking portal or IPC interfaces).

---

## Top-Level Runtime States: `IpcShortcutState`

```rust
pub enum IpcShortcutState {
    Initializing,
    Active {
        description: String,
    },
    CompositorManaged {
        binding_status: IpcCompositorBindingStatus,
        snippet: String,
        conflict: Option<String>,
        diagnostic: Option<String>,
    },
    Conflict {
        details: String,
    },
    Unavailable {
        reason: String,
    },
    Failed {
        error: String,
    },
}
```

### State Definitions

* **`Initializing`**: The worker thread has spawned and is negotiating initial backend registration.
* **`Active { description }`**: The shortcut is actively grabbed or registered. Applies to `Native` and `Portal` backends.
* **`CompositorManaged { binding_status, snippet, conflict, diagnostic }`**: Shortcut management is delegated to an external compositor. Contains the generated configuration snippet, the active binding status, and diagnostic messages.
* **`Conflict { details }`**: The shortcut collided with an existing application or compositor binding.
* **`Unavailable { reason }`**: The required platform subsystem is unavailable in this session.
* **`Failed { error }`**: An unrecoverable runtime error occurred on the worker thread.

---

## Compositor Binding States: `IpcCompositorBindingStatus`

When `state` is `IpcShortcutState::CompositorManaged`, `binding_status` classifies the live compositor inspection:

```rust
pub enum IpcCompositorBindingStatus {
    Verified,
    BoundUnverified,
    Unconfigured,
    Conflict,
}
```

### 1. `Verified`
The binding is confirmed present in the active compositor layout, and its command target is verified as invoking Pookie Paste (e.g. `exec pookie-paste --toggle`).

### 2. `BoundUnverified`
The key combination is actively bound in the running compositor, but Pookie cannot inspect or prove the command target over IPC.

* **Production Case**: Hyprland Lua callbacks (`__lua` dispatcher). When queried via `j/binds`, Hyprland reports that the key is actively bound to a Lua callback ID. The compositor claims the key, but Pookie cannot inspect the Lua closure bytecode over IPC to prove it invokes `pookie-paste`.
* **Key Invariant**:
  > **Binding existence can be observed; exact callback/command ownership cannot be proven.**
* **Accepted State**: In Pookie's progressive-disclosure UI, `BoundUnverified` is treated as a valid, configured state. It is not an error, not a conflict, and not unconfigured.
* **No False Upgrades**: Pookie intentionally does not upgrade `BoundUnverified` to `Verified` merely because an activation request later arrives over IPC.

### 3. `Unconfigured`
No binding for the configured shortcut exists in the active compositor configuration.

### 4. `Conflict`
The key combination is bound in the active compositor, but targets a different command or application.

---

## State Interpretation Table

| Capability | `IpcShortcutState` | `IpcCompositorBindingStatus` | Meaning | UI Attention Banner? | UI Setup View |
| --- | --- | :---: | --- | :---: | --- |
| **`Native`** | `Active` | *N/A* | X11 root-window grab active. | Hidden | Minimal `Current shortcut` row with `Change` button. |
| **`Native`** | `Conflict` | *N/A* | Key grab rejected (`BadAccess`). | Shown | Conflict warning and key recorder. |
| **`Portal`** | `Active` | *N/A* | Portal session bound and listening. | Hidden | Minimal `Current shortcut` row; clicking opens portal dialog. |
| **`CompositorManaged`** | `CompositorManaged` | `Verified` | Compositor binding proven for Pookie. | Hidden | Minimal `Current shortcut` row with `Change` button. |
| **`CompositorManaged`** | `CompositorManaged` | `BoundUnverified` | Bound in compositor (opaque Lua callback). | Hidden | Minimal `Current shortcut` row with `Change` button. |
| **`CompositorManaged`** | `CompositorManaged` | `Unconfigured` | Key missing from compositor configuration. | **Shown** (Warning) | Multi-step guide: snippet, Copy button, and "Check" button. |
| **`CompositorManaged`** | `CompositorManaged` | `Conflict` | Key bound to another command in compositor. | **Shown** (Conflict) | Multi-step guide: snippet, conflict notice, and re-record prompt. |
| **Any** | `Unavailable` | *N/A* | Backend cannot run on current session. | **Shown** (Warning) | Informational banner and unavailable notice. |
| **Any** | `Failed` | *N/A* | Worker encountered fatal error. | **Shown** (Error) | Error banner and failure details. |

---

## Refresh Semantics

1. **Passive Read (`GetShortcutStatus`)**: Reads directly from `ShortcutListener`'s in-memory `RwLock<ShortcutStatusInfo>` without I/O.
2. **Authoritative Recheck (`RecheckShortcutStatus`)**:
   * For compositor-managed backends, executes a live IPC query against the compositor (`GET_CONFIG` on Sway, `j/binds` on Hyprland).
   * Does not mutate `config.toml` or compositor configuration.
   * Updates in-memory status cache.
3. **Pre-Popup Bounded Recheck**: When the popup is opened via `ToggleUi`, the daemon runs a 50ms bounded recheck for compositor-managed backends. If the compositor query completes within 50ms, status is refreshed before rendering; if the query times out, UI launches fail-open without blocking.

---

## Important Invariants

1. **Truthful Verification**: Pookie reports `Verified` only when ownership is proven over live compositor IPC. It never reports `Verified` based solely on static file fallback or test mocks.
2. **No False Upgrades**: Pookie does not upgrade `BoundUnverified` to `Verified` simply because an activation IPC request (`ToggleUi`) arrived. An opaque Lua callback remains truthfully documented as `BoundUnverified`.
3. **Idempotence**: `recheck()` is strictly read-only and idempotent. Repeating `recheck()` never alters configuration files or compositor bindings.
