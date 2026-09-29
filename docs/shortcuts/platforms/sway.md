# Sway Compositor-Managed Shortcuts

This document describes the Sway compositor-managed shortcut implementation in Pookie Paste: live IPC configuration inspection, variable expansion, binding diagnosis, and setup workflows.

For the common architecture, see [Shortcut Overview](../overview.md).

---

## Ownership & Philosophy

In Sway (and wlroots compositors generally), external applications cannot arbitrarily grab global key combinations. Keybinding management belongs strictly to the compositor.

Pookie embraces this design:
* **The Compositor Owns the Binding**: Sway captures the key chord and executes `pookie-paste --toggle`.
* **Pookie Owns User Intent**: Pookie records the desired shortcut in `~/.config/pookie-paste/config.toml` and generates the corresponding configuration snippet.
* **Non-Interference Invariant**: Pookie **never** silently modifies `~/.config/sway/config` or `/etc/sway/config`. The user maintains complete control over their compositor configuration.

---

## Generated Binding Snippet

Based on the configured shortcut in `config.toml`, Pookie formats a snippet conforming to Sway's canonical `bindsym` syntax:

```sway
bindsym Mod4+v exec pookie-paste --toggle
```

Modifiers are mapped directly to canonical Sway modifier names (`Mod4`, `Ctrl`, `Mod1`, `Shift`), and keys are mapped to their lowercase XKB symbols (or named keys such as `space`, `Return`, `F12`).

> [!NOTE]
> While Pookie's parser supports reading user Sway configurations that define and use variable aliases such as `set $mod Mod4`, the generated snippet deliberately emits canonical `Mod4` so that it works independently of variable definitions in the user's configuration.

---

## Live IPC Inspection (`$SWAYSOCK` & `GET_CONFIG`)

To authoritatively verify whether the shortcut is bound and operational, [`SwayShortcutBackend`](../../../crates/daemon/src/sway_shortcut_backend.rs) communicates with the live compositor over Sway's Unix domain socket:

```rust
const MAGIC: &[u8; 6] = b"i3-ipc";
const GET_CONFIG: u32 = 9;
```

1. **Socket Discovery**: Reads the `$SWAYSOCK` environment variable.
2. **Query Execution**: Connects to the socket and sends an `i3-ipc` payload with message type `9` (`GET_CONFIG`).
3. **Response Parsing**: Receives and decodes the active configuration tree currently loaded by the running Sway instance.

### Variable Resolution & Matching
`diagnose_sway_config()` performs scoped analysis on the returned configuration text:
* **Variable Extraction**: Identifies variable definitions (e.g. `set $mod Mod4`).
* **Modifier Expansion**: Expands occurrences of `$mod` into its underlying modifier mask (e.g. `Mod4` / Super).
* **Command Verification**: Identifies `bindsym` lines matching the target key chord. If the command invokes `pookie-paste` (matching binary name, flag, or desktop execution), it is classified as targeting Pookie.

---

## Binding Status Diagnosis

The active configuration inspection produces one of three outcomes:

| Diagnosis | Meaning | Compositor Binding Status |
| --- | --- | :---: |
| **`MatchedPookie`** | Key is bound to `pookie-paste --toggle` in running Sway layout. | **`Verified`** |
| **`Conflict`** | Key is bound to a different command or script. | **`Conflict`** |
| **`NotFound`** | Key chord does not appear in active configuration. | **`Unconfigured`** |

Authoritative status diagnosis in production strictly requires live compositor IPC. If Sway IPC is unreachable, the check fails closed.

---

## Preflight Conflict Detection

When a user attempts to change their shortcut via the UI or `SetShortcut`:
1. `SwayShortcutBackend::check_conflict(candidate)` queries the active Sway configuration via IPC.
2. If the chord is occupied by another command, it returns `Ok(Some(command))`. `ReloadCoordinator` aborts with `ShortcutError::Conflict`, leaving `config.toml` untouched.
3. If the candidate is free or already belongs to Pookie, the check succeeds and `config.toml` is updated.
4. **Fail-Closed Rule**: If the IPC socket is unreachable, `check_conflict()` returns an error, preventing configuration changes based on unknown state.

---

## Recording Limitation in Wayland Compositors

> [!NOTE]
> When recording a shortcut in Pookie's UI, if Sway already has a binding configured for that chord (e.g. `Mod4+v`), Sway intercepts the key press and executes the associated command **before** Pookie's popup window receives the key event.
>
> This is a fundamental Wayland compositor security constraint, not a defect in Pookie Paste. To re-record an existing shortcut, the user may either edit `config.toml` directly or temporarily disable the binding in Sway.

---

## UI Setup Workflow

When Sway is detected, the UI shortcut setup view provides a structured progressive-disclosure workflow:

1. **Step 1 (Shortcut)**: Record or view desired shortcut.
2. **Step 2 (Add Binding)**: Displays the exact canonical `bindsym` snippet with a **Copy** button and an **Open Config** button (opening `~/.config/sway/config` or `/etc/sway/config` in the user's default desktop editor).
3. **Step 3 (Apply)**: Prompts the user to run `swaymsg reload` (or click the UI's **Reload Sway** button) and click **Check**.
4. **Happy-State Collapse**: As soon as `recheck()` reports `Verified`, the multi-step guide collapses into the minimal `Current shortcut` row with a `Change` button.
