# Pookie Paste

Pookie Paste is a fast, lightweight clipboard history manager for Linux.

It is inspired by Windows Clipboard History and is designed around a simple workflow:

1. Copy something.
2. Press `Super+V`.
3. Select an item from clipboard history.
4. Paste it directly into the previously active application.

## Current Status

Pookie Paste currently supports text clipboard history with:

- Persistent clipboard history
- `Super+V` global shortcut
- Searchable popup interface
- Direct paste on X11
- Direct paste on KDE Plasma Wayland
- Clipboard-only fallback where direct paste is unavailable
- Persistent SQLite storage
- Automatic startup after login
- Safe application updates
- User-local installation
- Clean uninstall and optional data purge

Image clipboard history is planned for a future release.

## Linux Support

Pookie Paste currently targets the following Linux distribution families:

- Debian / Ubuntu based distributions
- Fedora / RHEL based distributions
- Arch / Manjaro based distributions
- openSUSE based distributions

Desktop/session capabilities currently differ.

### X11

Direct paste is supported.

### KDE Plasma + Wayland

Direct paste is supported using:

- XDG Desktop Portal
- EIS/libei
- Pookie Paste KWin focus integration

### Other Wayland Desktops

Clipboard history can work, but direct paste currently falls back when a supported focus-restoration backend is unavailable.

Additional Wayland desktop support is planned.

## Install

### Latest Stable Release

Install the latest published Pookie Paste release:

```bash
curl -fsSL \
  https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh \
  | bash
```

The bootstrap installer:

1. Determines the latest stable GitHub release.
2. Downloads the installer files from that exact release tag.
3. Detects your Linux distribution and architecture.
4. Downloads the matching prebuilt Pookie Paste binary.
5. Verifies its SHA256 checksum.
6. Installs Pookie Paste into your user account.
7. Configures desktop startup.
8. Installs the KDE KWin helper when running KDE Plasma.
9. Starts Pookie Paste.

Rust is not required for normal installation.

### Install a Specific Version

Example:

```bash
curl -fsSL \
  https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh \
  | bash -s -- --version v0.1.0
```

You can also use:

```bash
curl -fsSL \
  https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh \
  | POOKIE_VERSION=v0.1.0 bash
```

## Developer Installation

Clone the repository:

```bash
git clone \
  https://github.com/riyanj220/pookie-paste.git

cd pookie-paste
```

Install from source:

```bash
./scripts/install.sh --from-source
```

This builds Pookie Paste locally using Cargo.

## Installed Files

Pookie Paste uses a user-local installation.

Binaries:

```text
~/.local/bin/pookie-paste
~/.local/bin/pookie-paste-ui
```

Desktop entry:

```text
~/.local/share/applications/io.github.riyanj220.PookiePaste.desktop
```

Autostart entry:

```text
~/.config/autostart/io.github.riyanj220.PookiePaste-autostart.desktop
```

Application data:

```text
~/.local/share/pookie-paste/
```

Application state:

```text
~/.local/state/pookie-paste/
```

On KDE Plasma, the KWin helper is installed through KDE's package system and normally resides under:

```text
~/.local/share/kwin/scripts/pookie-focus/
```

## Update

Running the installer again updates the application while preserving clipboard history and application state.

```bash
curl -fsSL \
  https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh \
  | bash
```

The new release is downloaded and verified before the currently installed Pookie Paste instance is stopped.

## Uninstall

From a cloned Pookie Paste repository:

```bash
./scripts/uninstall.sh
```

This removes:

- Pookie Paste binaries
- Desktop integration
- Autostart integration
- KDE KWin helper

Clipboard history and application state are preserved.

To also remove all Pookie Paste user data:

```bash
./scripts/uninstall.sh --purge
```

## Development

Development documentation:

[Development Guide](docs/DEVELOPMENT.md)

## Architecture

Pookie Paste uses a modular Rust workspace.

Major components include:

- Daemon
- Clipboard backends and watchers
- Processing engine
- History service
- SQLite storage
- IPC layer
- Popup UI
- Global shortcut integration
- Focus restoration
- Direct-paste backends

Detailed architecture:

[Architecture Documentation](docs/ARCHITECTURE.md)

## Roadmap

Project roadmap:

[Roadmap](docs/ROADMAP.md)

## Contributing

Contributions are welcome.

Please review the development documentation before contributing.

## License

Pookie Paste is licensed under the MIT License.

See [LICENSE](LICENSE).
