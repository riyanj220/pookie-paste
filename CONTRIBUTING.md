# Contributing to Pookie Paste

Thank you for your interest in contributing to Pookie Paste.

Pookie Paste is an open-source clipboard history manager for Linux focused on speed, reliability, native desktop integration, and a low-overhead user experience.

Contributions of all kinds are welcome, including bug fixes, performance improvements, platform compatibility enhancements, test coverage, and documentation refinements.

---

## Before You Start

For non-trivial changes or new features, please open an issue first to discuss your proposal. This ensures your approach aligns with the project's architecture, platform support goals, and core invariants before writing code.

Small fixes, documentation improvements, and straightforward bug fixes can be submitted directly as pull requests.

---

## Development Setup

Pookie Paste requires a Rust toolchain and Linux desktop development headers.

### Prerequisites

- Rust stable (edition 2024 / 1.85+)
- Cargo
- A Linux graphical session (X11 or a supported Wayland compositor)

Install Rust using `rustup` if needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Building the Project

Build the daemon and the popup UI:

```bash
cargo build -p daemon -p ui
```

To prepare desktop integration files, autostart entries, and platform helpers on your local machine, run the local source installer:

```bash
./scripts/install.sh --from-source
```

### Validation Commands

Before submitting code, ensure all workspace checks pass:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For complete local setup details and signal handling during development, see the [Development Guide](docs/DEVELOPMENT.md).

---

## Where to Read First

Pookie Paste uses a modular documentation system. Consult the relevant subsystem documentation before modifying code:

| Subsystem / Area | Documentation |
| :--- | :--- |
| **High-level architecture & invariants** | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) |
| **Process model, lifecycle & signals** | [`docs/architecture/runtime-model.md`](docs/architecture/runtime-model.md) |
| **Clipboard capture, normalization & storage** | [`docs/clipboard/overview.md`](docs/clipboard/overview.md) |
| **Focus restoration & synthetic paste** | [`docs/activation/overview.md`](docs/activation/overview.md) |
| **Global shortcut management & listener** | [`docs/shortcuts/overview.md`](docs/shortcuts/overview.md) |
| **Shortcut verification status model** | [`docs/shortcuts/status-model.md`](docs/shortcuts/status-model.md) |
| **IPC protocol, codec & request routing** | [`docs/ipc/overview.md`](docs/ipc/overview.md) |
| **Desktop matrix & known limitations** | [`docs/platform-support.md`](docs/platform-support.md) |

Each subsystem overview links to platform-specific implementation guides for **X11**, **KDE Plasma Wayland**, **Sway**, and **Hyprland**.

---

## Architecture & Invariants

Architectural changes must preserve the separation between the persistent daemon and ephemeral popup UI, platform abstraction boundaries, and documented subsystem invariants. Read [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) before making cross-cutting changes.

### Critical Invariants

Every change must uphold these project invariants:

1. **Target Confirmation Before Paste**: Synthetic paste (`Ctrl+V`) is never emitted into an unconfirmed focus target. If `target = None`, activation stops before paste capability evaluation and returns `ClipboardUpdated`. If focus restoration or confirmation fails, activation returns `PasteFailed` without injecting keystrokes.
2. **Self-Write Suppression**: Clipboard writes originating from Pookie register a SHA-256 fingerprint in memory so the subsequent platform change event is suppressed and never re-enters history.
3. **State Ownership**: Durable application state (database, images, runtime tokens, watchers, configuration) belongs strictly to the daemon. The popup UI contains only transient view state.
4. **Transactional X11 Shortcuts**: X11 passive root grabs must be rebound transactionally with automatic rollback to the previous binding on failure.
5. **KDE Portal Authority**: On KDE Plasma Wayland, the effective shortcut reported by the XDG Desktop Portal is authoritative over `config.toml`.
6. **Compositor Non-Interference**: Pookie records intent in `config.toml` but never silently edits external compositor files (`sway/config`, `hyprland.conf`). The compositor retains exclusive ownership over its keybindings.
7. **Truthful Shortcut Status**: Shortcut status must remain truthful. Never report `Verified` unless verified over live compositor IPC or authoritative portal state. Unverifiable bindings (such as Hyprland `__lua` callbacks) must remain `BoundUnverified`.

---

## Platform-Specific Changes

Pookie Paste isolates desktop differences behind trait abstractions (`FocusBackend`, `PasteBackend`, `ShortcutBackend`). Changes touching:

- Window focus capture or restoration
- Synthetic paste keystroke injection
- Clipboard event monitoring or data exchange
- Global shortcut registration or binding verification
- Compositor IPC or D-Bus communication

frequently behave differently across **X11**, **KDE Plasma Wayland**, **Sway**, and **Hyprland**.

When modifying platform-specific code:

1. **Identify Affected Backends**: Determine which platform crates and backend modules are impacted.
2. **Preserve Unaffected Platforms**: Verify that changes on one platform do not regress behavior or break compilation on others.
3. **Write Automated Regression Tests**: Use unit tests or mock server fixtures where available.
4. **Perform Real-Environment Manual Testing**: Automated tests alone cannot guarantee correct compositor interaction. Test on real desktop sessions whenever possible.

---

## Testing

Pookie Paste relies on a multi-tiered testing strategy:

- **Unit Tests**: Verify isolated business logic, text normalization, image hashing, protocol codecs, and policy limits.
- **Integration Tests**: Exercise database persistence, history promotion, IPC client/server request routing, and transaction rollbacks.
- **Platform Mock Tests**: Simulate compositor IPC responses (e.g. mock Sway IPC server, simulated Hyprland socket output) to verify parser and status logic without a live display server.
- **Manual End-to-End Validation**: Required for changes involving live window focus restoration, synthetic input emission, and desktop shortcut interception.

Every bug fix should ideally include an automated regression test. Run the test suite before submitting:

```bash
cargo test --workspace
```

---

## Pull Requests

When submitting a pull request, provide a clear and structured description:

- **Summary**: What changed and why.
- **Testing Done**: List the automated tests run and any manual validation performed.
- **Platform Coverage**: State explicitly which environments were tested:
  ```text
  Tested on:
  - X11: Verified with XTest paste and passive root grabs
  - KDE Wayland: Not tested
  - Sway: Tested with Sway 1.9 IPC
  - Hyprland: Not tested
  ```
  *(Only claim environments you actually validated).*
- **Compatibility & Limitations**: Note any known edge cases, compositor versions, or potential breaking changes.

Before opening a PR:
- [ ] Code formatted with `cargo fmt --all -- --check`
- [ ] Clippy passes with `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] All tests pass with `cargo test --workspace`
- [ ] Documentation updated if user-facing behavior, architecture, or configuration changed

---

## Branches & Commits

### Branch Naming

Use short, descriptive branch names indicating the area of work:

- `feature/global-shortcut-manager`
- `fix/sway-shortcut-status`
- `fix/activation-target-safety`
- `docs/architecture`
- `refactor/ipc-protocol`

### Commit Messages

Write clear, concise commit messages that describe the intent of the change:

- Use the imperative mood in the subject line (e.g., `Add transactional rollback to X11 shortcut backend`).
- Keep the subject line concise (under 72 characters).
- Explain the motivation and context in the body when the change is non-trivial.
- Avoid vague messages like `fix`, `wip`, `update`, or `cleanup`.

---

## Performance & Dependencies

### Performance Expectations

Pookie Paste operates continuously in the background and responds to latency-sensitive hotkeys. Changes should avoid:

- Unnecessary allocations or buffer clones on clipboard ingestion paths.
- Blocking file or socket I/O on async Tokio runtimes.
- Redundant SQLite queries during UI searches or history updates.
- Idle CPU usage or unbounded memory growth.

### Dependency Hygiene

Dependencies are added intentionally. Before introducing a new crate, evaluate:

- Can the functionality be achieved cleanly using existing workspace dependencies?
- Is the crate actively maintained with a clean security record?
- Does it significantly impact compile times or binary footprint?
- Does it maintain broad Linux distribution compatibility without heavy native runtime requirements?

---

## Documentation

Keep documentation in sync with code changes. Update documentation when modifying:

- Architecture or invariant rules (`docs/ARCHITECTURE.md`, `docs/architecture/`)
- Clipboard handling or storage (`docs/clipboard/overview.md`)
- Activation, focus, or paste mechanisms (`docs/activation/`)
- Shortcut listener, config, or status behavior (`docs/shortcuts/`)
- IPC protocol, requests, or responses (`docs/ipc/overview.md`)
- Platform support status or limitations (`docs/platform-support.md`)
- Installation scripts or development workflows (`README.md`, `docs/DEVELOPMENT.md`)

Place deep implementation details into the appropriate modular guide under `docs/` rather than duplicating them across top-level files.

---

## Reporting Issues

When reporting bugs, include sufficient context to reproduce the issue:

- **Linux Distribution**: (e.g. Arch Linux, Fedora 41, Ubuntu 24.04)
- **Session Type**: X11 or Wayland
- **Desktop Environment / Compositor**: (e.g. KDE Plasma 6.2, Sway 1.9, Hyprland 0.56)
- **Pookie Paste Version**: (e.g. `pookie-paste --version` or git commit hash)
- **Steps to Reproduce**: Minimal steps that trigger the problem.
- **Expected vs. Actual Behavior**: What should happen versus what actually occurred.
- **Logs / Output**: Daemon terminal output or relevant system logs.

> [!CAUTION]
> Never include passwords, access tokens, private keys, or sensitive clipboard contents in issue reports or public logs.

---

## Feature Requests

Feature proposals are welcomed. Pookie Paste aims to remain lightweight, focused, reliable, and low-overhead.

When submitting a feature proposal, describe:
- The problem or workflow friction you are addressing.
- The proposed user experience and interface behavior.
- Why the feature fits Pookie Paste's core scope as a native clipboard manager.
- Any alternative solutions or workarounds you considered.

---

## Code of Conduct

All contributors and participants are expected to adhere to the project's [Code of Conduct](CODE_OF_CONDUCT.md).

---

## License

By contributing to Pookie Paste, you agree that your contributions will be licensed under the project's [MIT License](LICENSE).
