# Development Guide

This guide is for contributors who want to clone Pookie Paste, build it, run it locally, and validate changes.

## Quick Start

Clone the repository:

```bash
git clone https://github.com/riyanj220/pookie-paste.git
cd pookie-paste
```

Build the daemon and popup UI:

```bash
cargo build -p daemon -p ui
```

Run the full validation suite:

```bash
cargo fmt --all -- --check

cargo check --workspace

cargo test --workspace

cargo clippy   --workspace   --all-targets   --   -D warnings
```

## Requirements

For normal Rust development you need:

- Git
- Rust
- Cargo
- A Linux graphical session

Install Rust with `rustup` if it is not already available:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Verify:

```bash
rustc --version
cargo --version
```

Pookie Paste also depends on native Linux desktop libraries. The project installer handles supported distribution dependencies automatically, so the easiest way to prepare and test a complete desktop installation is:

```bash
./scripts/install.sh --from-source
```

This builds Pookie Paste from the current checkout and installs the daemon, UI, desktop integration, autostart entry, and KDE helper when applicable.

## Running During Development

For fast local iteration, build both binaries:

```bash
cargo build -p daemon -p ui
```

Then run the daemon directly:

```bash
./target/debug/pookie-paste
```

Keep both binaries in the same Cargo output directory. The daemon launches `pookie-paste-ui` from beside its own executable when `Super+V` is activated.

Do not run a development daemon while another Pookie Paste daemon is already active.

You can check with:

```bash
pgrep -a pookie-paste
```

and stop an existing instance with:

```bash
pkill -TERM -x pookie-paste
```

### KDE Plasma Wayland

For KDE Plasma Wayland development, use the source installer at least once:

```bash
./scripts/install.sh --from-source
```

This installs the KWin focus helper and the desktop integration required for the complete Wayland direct-paste path.

After changing only Rust code, you can still build and run the local debug binaries for faster iteration.

## Project Structure

```text
crates/
├── daemon
├── ui
├── pookie-clipboard
├── pookie-core
├── history
├── storage
└── ipc
```

See [Architecture](ARCHITECTURE.md) for the responsibility of each crate and the X11/Wayland runtime flows.

## Development Workflow

Create a focused branch from the latest `main`:

```bash
git switch main
git pull origin main
git switch -c feat/<feature-name>
```

Make the change, add or update tests, then run:

```bash
cargo fmt --all

cargo check --workspace

cargo test --workspace

cargo clippy   --workspace   --all-targets   --   -D warnings

git diff --check
```

For changes involving clipboard capture, focus restoration, popup behavior, or direct paste, also test the behavior in a real graphical session.

Current primary validation targets are:

- X11
- KDE Plasma Wayland

## Installation and Release Testing

The normal source-install path is:

```bash
./scripts/install.sh --from-source
```

The isolated installation smoke test is:

```bash
./scripts/smoke-test-install.sh --from-source
```

KDE Plasma Wayland has an additional validation helper:

```bash
./scripts/validate-kde-wayland.sh
```

Release-specific validation is documented in [Release Testing](RELEASE_TESTING.md).

## Useful Documentation

- [Architecture](ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)
- [Release Testing](RELEASE_TESTING.md)
- [Contributing](../CONTRIBUTING.md)