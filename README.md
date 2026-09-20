# Pookie Paste

Tired of bloated, text-only clipboard managers on Linux with clunky workflows and no direct paste? There you go.

**Pookie Paste** is a lightweight, snappy clipboard manager inspired by **Windows Clipboard History**. Copy text or images, press `Super+V`, choose an item, and Pookie Paste immediately restores it and pastes it directly back into your active application.

## Demo
<img width="800" height="450" alt="ezgif-1812d21b3a903346" src="https://github.com/user-attachments/assets/edd33c24-8481-478e-b984-64552b240148" />

## Screenshot
<img width="1034" height="604" alt="Pookie Paste Screenshot" src="https://github.com/user-attachments/assets/d0265d10-8741-483e-b818-e11edefa699b" />

## Features

- **Text & Image Support** — Full clipboard history for both rich text and images with instant thumbnail previews.
- **Direct Paste** — Pastes directly into your target application on X11 and KDE Plasma Wayland with zero extra clicks.
- **`Super+V` Shortcut** — Snappy, keyboard-friendly popup positioned right near your active window.
- **Pin & Manage** — Pin favorite clips to the top, remove individual items, or clear history effortlessly.

## Platform Support

Pookie Paste currently targets:

- **X11** — supported
- **KDE Plasma + Wayland** — supported
- **Other Wayland desktops, including GNOME** — not currently supported for the full Pookie Paste experience

Support for additional Wayland desktop environments is planned.

## Install

Install the latest stable release:

```bash
curl -fsSL   https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh   | bash
```

Rust is not required for normal installation.

### Install a Specific Version

```bash
curl -fsSL   https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh   | bash -s -- --version <version>
```

Example:

```bash
curl -fsSL   https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh   | bash -s -- --version v0.1.1
```

## Update

Run the installer again:

```bash
curl -fsSL   https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh   | bash
```

Existing clipboard history and application state are preserved.

## Uninstall

From a cloned repository:

```bash
./scripts/uninstall.sh
```

This removes Pookie Paste while preserving clipboard history and application state.

To remove everything:

```bash
./scripts/uninstall.sh --purge
```

## Data Locations

Pookie Paste uses user-local Linux directories.

```text
~/.local/bin/pookie-paste
~/.local/bin/pookie-paste-ui

~/.local/share/pookie-paste/
├── pookie-paste.db
└── images/

~/.local/state/pookie-paste/
```

On KDE Plasma, the KWin focus helper is installed under:

```text
~/.local/share/kwin/scripts/pookie-focus/
```

XDG directory overrides are respected when configured.

## Development

Install from source:

```bash
git clone https://github.com/riyanj220/pookie-paste.git
cd pookie-paste
./scripts/install.sh --from-source
```

Project documentation:

- [Architecture](docs/ARCHITECTURE.md)
- [Development Guide](docs/DEVELOPMENT.md)
- [Roadmap](docs/ROADMAP.md)
- [Release Testing](docs/RELEASE_TESTING.md)

## Contributing

Contributions are welcome. Please read the [Development Guide](docs/DEVELOPMENT.md) before contributing.

## License

Pookie Paste is licensed under the [MIT License](LICENSE).
