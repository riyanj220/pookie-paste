# Pookie Paste

Pookie Paste is a lightweight clipboard history manager for Linux, inspired by **Windows Clipboard History**.

Copy text or images, press `Super+V`, choose an item, and Pookie Paste restores it to the clipboard and pastes it back into the previously active application when direct paste is supported.

## Demo
<img width="800" height="800" alt="ezgif-1812d21b3a903346" src="https://github.com/user-attachments/assets/8ec013de-61c6-41b7-9389-97fafcc39d94" />

## Screenshot
<img width="1919" height="1041" alt="ss 1" src="https://github.com/user-attachments/assets/70f570af-ecb7-4006-88e0-4b835816b5ac" />

## Features

- Text and image clipboard history
- `Super+V` global shortcut
- Persistent history across restarts
- Mixed text/image popup with image thumbnails
- Keyboard and mouse navigation
- Direct paste on X11
- Direct paste on KDE Plasma Wayland
- Safe clipboard-only fallback when direct paste is unavailable
- Automatic startup after login
- User-local installation
- Update, uninstall, and optional full data purge

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
