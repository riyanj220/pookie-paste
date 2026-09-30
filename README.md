# Pookie Paste

Tired of clunky, text-only clipboard managers on Linux?

**Pookie Paste** is a lightweight, snappy clipboard manager inspired by **Windows Clipboard History**. Copy text or images, press your shortcut (default `Super+V`), select an item, and Pookie Paste restores and pastes it directly back into your active application on supported environments.

## Demo

<img width="800" height="450" alt="ezgif-1812d21b3a903346" src="https://github.com/user-attachments/assets/edd33c24-8481-478e-b984-64552b240148" />

## Screenshot

<img width="1034" height="604" alt="Pookie Paste Screenshot" src="https://github.com/user-attachments/assets/d0265d10-8741-483e-b818-e11edefa699b" />

## Features

- **Text & Image Support** — Full clipboard history for rich text and images with instant thumbnail previews.
- **Direct Paste** — Restores and pastes the selected item directly into your active application on supported environments.
- **Configurable Shortcut** — Snappy, keyboard-driven access via a global hotkey (defaults to `Super+V`).
- **Pin & Manage** — Pin favorite clips to the top, remove individual items, or clear history effortlessly.
- **Cross-Desktop Native** — First-class desktop integration across X11, KDE Plasma Wayland, Sway, and Hyprland.
- **Lightweight & Fast** — Instant native popup built with egui, backed by an efficient background daemon.

## Platform Support

| Desktop / Platform | Support Status |
| :--- | :--- |
| **X11** | Supported |
| **KDE Plasma** (Wayland) | Supported |
| **Sway** | Supported |
| **Hyprland** | Supported |
| **Other Wayland** | Limited / clipboard-only fallback |

On environments without direct paste or window focus restoration, Pookie Paste gracefully falls back to clipboard-only mode so you can paste manually.

For technical details on focus backends, paste injection mechanisms, and shortcut models, see the [Platform Support Guide](docs/platform-support.md).

## Install

Install the latest stable release:

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh | bash
```

Rust is not required for normal installation (prebuilt x86_64 binaries are downloaded automatically).

### Install a Specific Version

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh | bash -s -- --version <version>
```

Example:

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh | bash -s -- --version v0.2.1
```

## Update

Run the installer again to update to the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh | bash
```

Existing clipboard history and application state are preserved.

## Uninstall

Remove Pookie Paste while keeping your settings and clipboard history:

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/uninstall.sh | bash
```

To remove Pookie Paste and all stored settings and clipboard data:

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/uninstall.sh | bash -s -- --purge
```

If you cloned the repository, you can also uninstall locally:

```bash
./scripts/uninstall.sh
```

## Documentation

Comprehensive architecture and subsystem documentation is available in `docs/`:

- [Architecture Overview](docs/ARCHITECTURE.md)
- [Platform Support Matrix](docs/platform-support.md)
- [Development Guide](docs/DEVELOPMENT.md)
- [Release Testing](docs/RELEASE_TESTING.md)

## Development

Build and install from source:

```bash
git clone https://github.com/riyanj220/pookie-paste.git
cd pookie-paste
./scripts/install.sh --from-source
```

For test suites, validation commands, and local development workflows, see the [Development Guide](docs/DEVELOPMENT.md).

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Development Guide](docs/DEVELOPMENT.md) before submitting pull requests.

## License

Pookie Paste is licensed under the [MIT License](LICENSE).
