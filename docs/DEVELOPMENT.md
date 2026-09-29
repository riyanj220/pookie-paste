# Development Guide

This guide covers local environment setup, building, running debug binaries, platform-specific development notes, and validating changes across Pookie Paste's supported desktop environments.

---

## Quick Start

Clone the repository and build the workspace binaries:

```bash
git clone https://github.com/riyanj220/pookie-paste.git
cd pookie-paste

cargo build -p daemon -p ui
```

Before submitting changes, run the full validation suite:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

---

## Requirements

Developing Pookie Paste requires:

- **Git**
- **Rust stable** (Rust edition 2024 / 1.85+)
- **Cargo**
- **Linux graphical session** (X11 or a supported Wayland compositor)

Install Rust via `rustup` if needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Pookie Paste relies on native Linux desktop libraries (X11, Wayland client libraries, and D-Bus headers). When building on a fresh machine, the project source installer automatically resolves package requirements across supported distribution families (Debian/Ubuntu, Fedora/RHEL, Arch, and openSUSE):

```bash
./scripts/install.sh --from-source
```

---

## Build & Validation

### Local Builds

For fast day-to-day compilation during Rust development:

```bash
# Build debug binaries
cargo build -p daemon -p ui

# Quick type checking across all workspace crates
cargo check --workspace

# Run tests for a specific crate
cargo test -p daemon
cargo test -p ipc
```

### Full Validation Suite

The project enforces strict formatting, linting, and test passes in CI:

```bash
# Verify code formatting
cargo fmt --all -- --check

# Workspace-wide compilation check
cargo check --workspace

# Linter checks (deny warnings)
cargo clippy --workspace --all-targets -- -D warnings

# Execute all workspace tests
cargo test --workspace
```

---

## Running Locally

### Direct Debug Execution

During active development, run the daemon directly from Cargo's output directory:

```bash
cargo build -p daemon -p ui
./target/debug/pookie-paste
```

> [!NOTE]
> The daemon locates the popup UI binary (`pookie-paste-ui`) beside its own executable. Both binaries must reside in the same directory (`target/debug/` for local builds, or `~/.local/bin/` for installed builds).

Enable debug tracing output by setting `RUST_LOG`:

```bash
RUST_LOG=debug ./target/debug/pookie-paste
```

### Single-Instance Behavior

Pookie Paste enforces a single running daemon per user session via its local Unix domain stream socket:

- If you attempt to start a development daemon while an existing daemon is already running, the new process logs an informational message and exits cleanly without altering state.
- Check for running daemon instances:
  ```bash
  pgrep -a pookie-paste
  ```
- Terminate a running daemon instance:
  ```bash
  pkill -TERM -x pookie-paste
  ```

Direct execution of `./target/debug/pookie-paste` is ideal for rapid Rust iteration, but it does not install desktop files, autostart entries, or the KDE KWin helper script.

---

## Desktop Integration Setup

To test the complete end-to-end desktop experience from your working tree (including application menus, autostart, and platform helpers), install directly from source:

```bash
./scripts/install.sh --from-source
```

This command:
1. Builds optimized release binaries (`target/release/pookie-paste` and `pookie-paste-ui`).
2. Installs executables into `~/.local/bin/`.
3. Installs desktop entries into `~/.local/share/applications/`.
4. Configures session autostart in `~/.config/autostart/`.
5. On KDE Plasma sessions, installs and activates the KWin focus helper script (`~/.local/share/kwin/scripts/pookie-focus/`).
6. Starts the newly built daemon in your active session.

---

## Platform Development Notes

Pookie Paste uses modular platform backends for window focus, paste injection, and global shortcuts.

### 1. X11
- Communicates directly with the display server via `x11rb`.
- Focus capture and restoration use `_NET_ACTIVE_WINDOW` root-window messages.
- Paste injection uses the `XTest` extension to emit synthetic Ctrl+V key events.
- Global shortcuts are directly owned by the daemon via transactional passive root grabs (`XGrabKey`).
- *Details: [X11 Activation](activation/platforms/x11.md) | [X11 Shortcuts](shortcuts/platforms/x11.md)*

### 2. KDE Plasma (Wayland)
- Interacts via session D-Bus and standard Wayland protocols.
- Focus capture and restoration require the KWin helper script (`pookie-focus`). Run `./scripts/install.sh --from-source` at least once to ensure the helper is installed and registered.
- Paste injection uses XDG Desktop Portal `RemoteDesktop` and the Emulated Input System (EIS / `libei`).
- Global shortcuts are managed via `org.freedesktop.portal.GlobalShortcuts`. The portal-reported shortcut is authoritative over `config.toml`.
- *Details: [KDE Wayland Activation](activation/platforms/kde-wayland.md) | [KDE Desktop Portal Shortcuts](shortcuts/platforms/kde-portal.md)*

### 3. Sway (wlroots)
- Communicates with the live compositor over `$SWAYSOCK` IPC.
- Focus restoration uses Sway IPC (`GET_TREE` and `[con_id=...] focus`).
- Paste injection uses the `zwp_virtual_keyboard_v1` protocol.
- Global shortcuts are compositor-managed. Sway owns the keybinding and executes `pookie-paste --toggle`. Pookie records user intent in `config.toml` and inspects `GET_CONFIG` over live IPC, but never modifies `~/.config/sway/config`. To test shortcut triggering locally, add the recommended `bindsym` directive to your Sway configuration and reload Sway (`swaymsg reload`).
- *Details: [Sway Activation](activation/platforms/sway.md) | [Sway Shortcuts](shortcuts/platforms/sway.md)*

### 4. Hyprland
- Communicates over the Unix domain command socket (`$HYPRLAND_INSTANCE_SIGNATURE`).
- Focus restoration uses Hyprland IPC window address inspection (`j/activewindow`) and dispatchers.
- Paste injection uses the `zwp_virtual_keyboard_v1` protocol.
- Global shortcuts are compositor-managed. Hyprland captures the hotkey and executes `pookie-paste --toggle`. Pookie inspects live bindings via `j/binds` IPC. If bound to an opaque Lua callback (`__lua`), the status is classified as `BoundUnverified`. Pookie never alters `hyprland.conf` or `hyprland.lua`.
- *Details: [Hyprland Activation](activation/platforms/hyprland.md) | [Hyprland Shortcuts](shortcuts/platforms/hyprland.md)*

---

## Real-Environment Validation

Automated unit tests and mock server fixtures verify parsing, serialization, and status logic. Changes affecting desktop integration should also be validated in real graphical sessions on the affected platforms whenever possible.

Current supported validation environments are:

- **X11**
- **KDE Plasma Wayland**
- **Sway**
- **Hyprland**

Specifically, manual session validation is recommended when modifying:
- Clipboard watcher event loops or format negotiation
- Window identity capture and focus restoration polling
- Synthetic keystroke injection timing and modifier state
- Popup UI placement, window flags, or focus grabbing
- Global shortcut registration, portal dialogues, or compositor reloading

---

## Project Layout

The repository is organized into focused workspace crates:

```text
crates/
├── daemon            # Persistent daemon, platform resolvers, backends, IPC server, CLI
├── ui                # Ephemeral popup GUI (egui / eframe)
├── pookie-clipboard  # X11 & Wayland clipboard watchers and MIME processors
├── pookie-core       # Domain models, text normalization, image hashing, policy limits
├── history           # Clipboard history service, deduplication, promotion, eviction
├── storage           # SQLite repository and atomic filesystem image store
└── ipc               # Unix domain socket transport, newline JSON codec, wire protocol
```

For crate responsibilities and interaction flows, see [Architecture Overview](ARCHITECTURE.md) and [Runtime Model](architecture/runtime-model.md).

---

## Common Development Workflow

A typical technical workflow for implementing changes:

1. **Build Binaries**:
   ```bash
   cargo build -p daemon -p ui
   ```
2. **Implement Targeted Changes**:
   Modify the relevant crate while respecting module boundaries and invariants.
3. **Execute Focused Unit & Integration Tests**:
   ```bash
   cargo test -p <affected_crate>
   ```
4. **Run Full Workspace Validation**:
   ```bash
   cargo fmt --all -- --check
   cargo check --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
5. **Validate in Live Desktop Session**:
   Test on your active desktop environment (X11, KDE, Sway, or Hyprland).
6. **Review Changes**:
   ```bash
   git diff
   ```

---

## Installation & Release Validation

Pookie Paste provides dedicated test scripts for verifying installation and packaging:

- **Source Installation**:
  ```bash
  ./scripts/install.sh --from-source
  ```
  Standard source build and local user installation.

- **Sandboxed Installation Smoke Test**:
  ```bash
  ./scripts/smoke-test-install.sh --from-source
  ```
  Validates the full installation lifecycle inside an isolated temporary directory (`HOME` and `XDG_*` redirected). Tests file permissions, artifact paths, and clean uninstallation without modifying host application data.

- **KDE Plasma Wayland Verification**:
  ```bash
  ./scripts/validate-kde-wayland.sh
  ```
  Platform-specific validation script for KDE Plasma Wayland sessions. Verifies KWin scripting environment, plugin registration, portal D-Bus endpoints, and token generation.

For formal release qualification procedures, see [Release Testing](RELEASE_TESTING.md).

---

## Debugging & Useful Commands

Useful commands when testing and debugging running instances:

```bash
# Check if the daemon is currently running
pgrep -a pookie-paste

# Trigger popup display through the running daemon
pookie-paste --toggle

# Inspect live shortcut configuration, capabilities, and binding status
pookie-paste --shortcut-status

# Request strict reload of config.toml in the running daemon
pookie-paste --reload

# View daemon and UI process logs with debug tracing enabled
RUST_LOG=debug ./target/debug/pookie-paste

# Gracefully terminate the running daemon
pkill -TERM -x pookie-paste
```

---

## Related Documentation

- [**Architecture Overview**](ARCHITECTURE.md) — High-level system architecture and invariants
- [**Runtime Model & Process Lifecycle**](architecture/runtime-model.md) — Daemon startup, shutdown, and UI launching
- [**Platform Support Matrix**](platform-support.md) — Compatibility matrix and compositor capabilities
- [**Clipboard Subsystem**](clipboard/overview.md) — Clipboard readers, watchers, and formats
- [**Activation & Focus**](activation/overview.md) — Focus restoration and direct paste lifecycle
- [**Global Shortcuts**](shortcuts/overview.md) — Cross-desktop shortcut subsystem
- [**IPC Subsystem**](ipc/overview.md) — Socket transport and request/response protocols
- [**Contributor Guide**](../CONTRIBUTING.md) — PR expectations, testing philosophy, and branch conventions
- [**Release Testing**](RELEASE_TESTING.md) — Release validation and qualification steps