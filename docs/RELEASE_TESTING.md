# Pookie Paste Release Testing

This document defines the validation process used before publishing or approving a Pookie Paste Linux release.

The purpose is to ensure that installation, startup, clipboard integration, direct paste, updates, uninstall, and data preservation behave consistently across supported Linux environments.

---

## 1. Automated Installation Smoke Test

Pookie Paste provides:

```bash
./scripts/smoke-test-install.sh
```

The smoke test validates the generic installation lifecycle in an isolated temporary user environment.

It does not use the normal Pookie Paste application data directories.

The test redirects:

```text
HOME
XDG_DATA_HOME
XDG_CONFIG_HOME
XDG_STATE_HOME
XDG_RUNTIME_DIR
```

into a temporary directory.

This protects the tester's normal Pookie Paste database and application state.

### Requirements

The test must be run:

- On Linux
- From a graphical X11 or Wayland session
- With no existing `pookie-paste` daemon running

Because the application process is currently identified by process name, the smoke test refuses to start while another Pookie Paste daemon is running.

---

## 2. Test Latest Prebuilt Release

Stop the normal Pookie Paste instance first.

Then run:

```bash
./scripts/smoke-test-install.sh
```

The test uses the public bootstrap installer and the latest stable GitHub release.

---

## 3. Test a Specific Release

Example:

```bash
./scripts/smoke-test-install.sh \
  --version v0.1.0
```

This validates the public bootstrap path using exactly that release.

---

## 4. Test Source Installation

Run:

```bash
./scripts/smoke-test-install.sh \
  --from-source
```

This validates the developer installation path:

```text
scripts/install.sh --from-source
```

instead of the prebuilt release path.

---

## 5. Preserve the Temporary Environment

For debugging a failed test:

```bash
./scripts/smoke-test-install.sh \
  --keep-temp
```

The script prints the temporary directory when it exits.

This makes it possible to inspect:

```text
installed binaries
desktop files
autostart files
database
state
startup logs
runtime socket
```

after the test.

---

## 6. Automated Smoke-Test Coverage

The automated smoke test verifies:

### Installation

- Pookie daemon binary is installed
- Pookie UI binary is installed
- Desktop entry is installed
- Autostart entry is installed
- Data directory is created
- State directory is created
- Startup log is created

### Runtime

- Daemon remains running
- SQLite database is created
- IPC Unix socket is created

### Standard Uninstall

- Daemon stops
- Binaries are removed
- Desktop entry is removed
- Autostart entry is removed
- Database remains present
- Application state remains present

### Reinstallation

- Binaries are installed again
- Existing database survives
- Daemon starts again
- IPC socket is recreated

### Purge

- Daemon stops
- Binaries are removed
- Application data is deleted
- Application state is deleted

---

## 7. What the Automated Smoke Test Does Not Validate

The isolated automated smoke test intentionally does not validate desktop-specific behavior.

The following require separate real-session testing:

- `Super+V` portal registration
- Popup positioning
- Popup focus behavior
- X11 focus restoration
- X11 direct paste
- KDE Plasma Wayland focus capture
- KDE Plasma Wayland focus restoration
- KWin helper installation
- KWin shortcut registration
- Portal/EIS direct paste
- GNOME Wayland fallback behavior
- Other compositor behavior

These are covered by the platform validation matrix below.

---

# Platform Validation Matrix

## Fedora KDE Plasma Wayland

Validate:

- Bootstrap installation
- Autostart
- `Super+V`
- Clipboard history popup
- Focus capture
- Focus restoration
- Portal/EIS direct paste
- KWin helper installation
- Restart after login
- Update
- Standard uninstall
- Purge

Expected direct-paste capability:

```text
Direct
```

---

## Ubuntu / Debian Family

Validate:

- Bootstrap installation
- Dependency installation
- Desktop entry
- Autostart
- Clipboard monitoring
- Global shortcut behavior
- Uninstall lifecycle

On GNOME Wayland, direct paste may fall back when no supported focus-restoration backend is available.

Expected behavior must be recorded during testing rather than assumed.

---

## Arch / Manjaro Family

Validate:

- `pacman` dependency handling
- Bootstrap installation
- Desktop integration
- KDE helper installation when Plasma is used
- Clipboard history
- Direct paste
- Update
- Uninstall

---

## openSUSE Family

Validate:

- `zypper` dependency handling
- Bootstrap installation
- Desktop integration
- Plasma behavior when applicable
- Clipboard history
- Update
- Uninstall

---

# Session Validation

## X11

Expected:

```text
Clipboard monitoring: supported
Focus restoration: supported
Direct paste: supported
```

Verify:

1. Focus an application.
2. Copy text.
3. Press `Super+V`.
4. Select a history item.
5. Confirm the original application regains focus.
6. Confirm the selected text is pasted directly.

---

## KDE Plasma Wayland

Expected:

```text
Clipboard monitoring: supported
KWin focus helper: supported
Focus restoration: supported
Portal/EIS direct paste: supported
```

Verify:

1. `pookie-focus` is installed.
2. KWin helper is enabled.
3. Focus a text application.
4. Press `Super+V`.
5. Select a clipboard entry.
6. Confirm the original KDE window regains focus.
7. Confirm direct paste occurs.

---

## Non-KDE Wayland

Expected behavior depends on available focus-restoration support.

Verify:

- Clipboard history works
- Popup opens
- Selected history item is written to the clipboard
- Application does not inject input into an unconfirmed target

A clipboard-only fallback is acceptable where direct paste cannot be safely performed.

---

# Release Artifact Validation

Before publishing a release, verify:

```bash
sha256sum -c SHA256SUMS
```

Expected:

```text
pookie-paste-<version>-linux-x86_64.tar.gz: OK
```

Inspect the archive:

```bash
tar -tzf \
  pookie-paste-<version>-linux-x86_64.tar.gz
```

The release must contain:

```text
bin/pookie-paste
bin/pookie-paste-ui

share/applications/
share/autostart/

share/pookie-paste/kwin/pookie-focus/

LICENSE
README.md
RELEASE_VERSION
```

---

# Runtime Compatibility

Inspect dynamic dependencies:

```bash
ldd pookie-paste
ldd pookie-paste-ui
```

Inspect required glibc versions:

```bash
objdump -T pookie-paste \
  | grep GLIBC_ \
  | sort -V

objdump -T pookie-paste-ui \
  | grep GLIBC_ \
  | sort -V
```

The highest required glibc symbol version should be recorded for each published binary release.

Do not claim compatibility with Linux distributions older than the verified runtime baseline.

---

# Release Approval Checklist

Before considering a release validated:

-  CI passes
-  Release workflow passes
-  SHA256 verification passes
-  Automated installation smoke test passes
-  Source-install smoke test passes
-  Fedora KDE Wayland test passes
-  X11 test passes
-  At least one Debian/Ubuntu-family test passes
-  At least one Arch-family test passes
-  openSUSE test completed or explicitly marked unverified
-  Non-KDE Wayland fallback tested
-  Update preserves database
-  Standard uninstall preserves data
-  Purge removes data
-  Runtime/glibc baseline recorded
-  README installation instructions match released behavior

A release should only be described as supported on environments that have actually passed the relevant validation.
