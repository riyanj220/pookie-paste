<p align="center">
  <img src="packaging/icons/hicolor/128x128/apps/io.github.riyanj220.PookiePaste.png" width="80" height="80" alt="Pookie Paste icon">
</p>

<h1 align="center">Pookie Paste</h1>

<p align="center">
  <a href="#documentation">Documentation</a> &bull;
  <a href="https://github.com/riyanj220/pookie-paste/issues/new?labels=bug">Report Bug</a> &bull;
  <a href="https://github.com/riyanj220/pookie-paste/discussions/new?category=ideas">Request Feature</a>
</p>

<p align="center">
  <strong>Clipboard history for Linux. Direct paste. Nothing unnecessary.</strong>
</p>

<p align="center">
  <a href="#quick-install"><img src="https://img.shields.io/badge/INSTALL-00a2ed?style=for-the-badge" alt="Install"></a>
  &nbsp;
  <a href="#features"><img src="https://img.shields.io/badge/FEATURES-f77f00?style=for-the-badge" alt="Features"></a>
  &nbsp;
  <a href="#platform-support"><img src="https://img.shields.io/badge/PLATFORM_SUPPORT-00a2ed?style=for-the-badge" alt="Platform Support"></a>
  &nbsp;
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/POWERED_BY-RUST-black?style=for-the-badge&logo=rust&logoColor=white" alt="Powered By: Rust"></a>
</p>


<p align="center">
  <img src="docs/assets/pookie-paste demo.gif" alt="Pookie Paste Rapid fire Demo">
</p>

---

## Quick Install

Install the latest stable release:

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh | bash
```

## Features

- **Direct Paste** — Select a clip and paste it straight back into your active application.
- **Easy Shortcut Management** — Open Pookie Paste instantly with `Super+V`, with guided shortcut setup across supported desktops.
- **Text & Image Support** — Keep text and images in the same clipboard history with instant thumbnail previews.
- **Pin & Manage** — Pin important clips, remove individual items, or clear your history when you want.
- **Privacy First** — Your clipboard history stays on your machine. No cloud account or remote clipboard service required.
- **Lightweight & Fast** — A focused native popup and efficient background daemon without unnecessary extras.

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

## Installation & Maintenance

### Install a Specific Version

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh | bash -s -- --version <version>
```

Example:

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh | bash -s -- --version v0.2.1
```

### Update

Run the installer again to update to the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/riyanj220/pookie-paste/main/install.sh | bash
```

Existing clipboard history and application state are preserved.

### Uninstall

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
