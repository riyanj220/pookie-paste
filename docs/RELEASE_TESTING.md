# Pookie Paste Release Testing

This document defines the validation process used before publishing a Pookie Paste Linux release.

The goal is to verify installation, startup, clipboard history, direct paste, updates, uninstall behavior, data preservation, and release artifacts on the platforms Pookie Paste currently supports.

Current primary validation targets:

- X11
- KDE Plasma Wayland

---

## 1. Automated Installation Smoke Test

Pookie Paste provides:

```bash
./scripts/smoke-test-install.sh
```

The smoke test validates the generic installation lifecycle in an isolated temporary user environment.

It redirects:

```text
HOME
XDG_DATA_HOME
XDG_CONFIG_HOME
XDG_STATE_HOME
```

into a temporary directory.

The real `XDG_RUNTIME_DIR` is preserved so the daemon can still access the current graphical X11 or Wayland session.

This protects the tester's normal Pookie Paste database, image history, and application state.

### Requirements

Run the smoke test:

- On Linux
- From a graphical X11 or Wayland session
- With no existing `pookie-paste` daemon running

The smoke test refuses to start while another Pookie Paste daemon is active.

---

## 2. Test Source Installation

Before publishing a release, validate the current source tree:

```bash
./scripts/smoke-test-install.sh   --from-source
```

This tests the developer installation path:

```text
scripts/install.sh --from-source
```

Use this before creating the final release artifact.

---

## 3. Test the Published Release

After publishing the release, validate the exact version users will install:

```bash
./scripts/smoke-test-install.sh   --version <version>
```

Example:

```bash
./scripts/smoke-test-install.sh   --version v0.2.0
```

To test the latest stable release:

```bash
./scripts/smoke-test-install.sh
```

---

## 4. Preserve the Temporary Environment

For debugging a failed smoke test:

```bash
./scripts/smoke-test-install.sh   --keep-temp
```

The script prints the temporary test directory when it exits.

Useful paths to inspect include:

```text
installed binaries
desktop files
autostart files
database
images/
application state
startup logs
runtime socket
```

---

## 5. Automated Smoke-Test Coverage

The automated smoke test verifies the generic installation lifecycle.

### Installation

- Pookie daemon binary is installed
- Pookie UI binary is installed
- Desktop entry is installed
- Autostart entry is installed
- Application data directory is created
- Application state directory is created
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
- Application data is preserved
- Clipboard database is preserved
- Stored image history is preserved
- Application state is preserved

### Reinstallation

- Binaries are installed again
- Existing clipboard history survives
- Existing image history survives
- Daemon starts again
- IPC socket is recreated

### Purge

- Daemon stops
- Binaries are removed
- Application data is deleted
- Stored image files are deleted
- Application state is deleted

---

## 6. What the Smoke Test Does Not Validate

The isolated smoke test intentionally does not validate desktop-specific interaction.

The following require real-session testing:

- `Super+V`
- Popup positioning
- Popup focus behavior
- Keyboard and mouse navigation
- Text activation
- Image activation
- Image thumbnail rendering
- Focus restoration
- X11 direct paste
- KDE Plasma Wayland focus capture
- KDE Plasma Wayland focus restoration
- KWin helper behavior
- Portal/EIS direct paste
- Mixed text/image history behavior

These are covered by the real-session validation below.

---

# Real-Session Validation

## X11

Expected capabilities:

```text
Text capture: supported
Image capture: supported
Focus restoration: supported
Direct paste: supported
```

Validate:

1. Start Pookie Paste.
2. Copy text.
3. Copy an image.
4. Press `Super+V`.
5. Confirm mixed text/image history appears correctly.
6. Confirm image thumbnails render with the correct aspect ratio.
7. Navigate using keyboard arrows.
8. Select entries using the mouse.
9. Activate a text item and confirm:
   - the original application regains focus
   - the selected text pastes directly
10. Activate an image in an image-capable target and confirm:
   - the original application regains focus
   - the selected image pastes directly
11. Confirm activated items move to the most-recent position.
12. Confirm self-generated clipboard writes do not create duplicate history rows.
13. Restart Pookie Paste and confirm text and image history still exists.
14. Confirm the popup can be opened repeatedly after activation without leaving a stale UI process.

For image testing, use an application that accepts pasted images.

A plain text editor rejecting an image is expected behavior.

---

## KDE Plasma Wayland

Expected capabilities:

```text
Text capture: supported
Image capture: supported
KWin focus helper: supported
Focus restoration: supported
Portal/EIS direct paste: supported
```

Validate:

1. Confirm `pookie-focus` is installed and enabled.
2. Start Pookie Paste.
3. Copy text.
4. Copy an image.
5. Press `Super+V`.
6. Confirm mixed text/image history appears correctly.
7. Confirm image thumbnails render correctly.
8. Navigate using keyboard and mouse.
9. Activate text and confirm focus restoration + direct paste.
10. Activate an image in an image-capable target and confirm focus restoration + direct paste.
11. Confirm activated items are promoted without creating duplicates.
12. Restart Pookie Paste and confirm mixed history persists.

Also validate clipboard-content transitions:

```text
image → text
text → image
image → text → image
activate old image → copy fresh text
activate old text → copy fresh image
```

The daemon must not repeatedly report stale MIME errors such as:

```text
clipboard payload is empty mime=image/png
```

If focus restoration cannot confirm the intended target, direct paste must be aborted rather than injected into another application.

---

## Known KDE Focus Edge Case

Temporary Plasma surfaces such as the application launcher can become the active KWin window immediately before `Super+V`.

If Pookie captures such a temporary target and that target disappears before activation, focus restoration may fail.

Expected safe behavior:

```text
focus target no longer exists
→ activation reports paste failure
→ direct input is not injected into another window
```

This is currently treated as a focus-selection edge case rather than a release-blocking safety issue, provided the failure remains safe.

---

# Mixed Content Validation

Before release, verify this sequence on both X11 and KDE Plasma Wayland:

```text
copy text A
copy image A
copy text B
copy image B
```

Open `Super+V`.

Expected order:

```text
image B
text B
image A
text A
```

Then:

1. Activate `image A`.
2. Open `Super+V` again.
3. Confirm `image A` is now the most recent item.
4. Confirm no duplicate image row was created.
5. Activate `text A`.
6. Confirm text activation still works correctly after image activation.

---

# Persistence Validation

Create mixed history containing both text and images.

Stop and restart Pookie Paste.

Verify:

- SQLite history survives
- Image files survive
- Image rows still reference valid files
- Thumbnails reload
- Old text items still activate
- Old image items still activate

Expected data layout:

```text
$XDG_DATA_HOME/pookie-paste/
├── pookie-paste.db
└── images/
    └── <uuid>.png
```

When `XDG_DATA_HOME` is unset, the normal fallback is:

```text
~/.local/share/pookie-paste/
```

---

# Release Artifact Validation

Before publishing a release, verify the generated checksum:

```bash
cd dist
sha256sum -c SHA256SUMS
```

Expected:

```text
pookie-paste-<version>-linux-x86_64.tar.gz: OK
```

Inspect the archive:

```bash
tar -tzf   pookie-paste-<version>-linux-x86_64.tar.gz
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

Runtime-created clipboard history and image files must not be included in the release archive.

---

# Runtime Compatibility

Inspect dynamic dependencies:

```bash
ldd pookie-paste
ldd pookie-paste-ui
```

Inspect required glibc symbol versions:

```bash
objdump -T pookie-paste   | grep GLIBC_   | sort -V

objdump -T pookie-paste-ui   | grep GLIBC_   | sort -V
```

Record the highest required glibc version for published binaries.

Do not claim compatibility with distributions older than the verified runtime baseline.

---

# Release Approval Checklist

Before considering a release validated:

- [ ] CI passes
- [ ] Release workflow passes
- [ ] `cargo check --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] Workspace Clippy passes with warnings denied
- [ ] Source-install smoke test passes
- [ ] Published-release smoke test passes
- [ ] SHA256 verification passes
- [ ] X11 text + image E2E validation passes
- [ ] KDE Plasma Wayland text + image E2E validation passes
- [ ] Mixed text/image history behaves correctly
- [ ] Image activation does not create duplicate history rows
- [ ] Update preserves database and image history
- [ ] Standard uninstall preserves application data
- [ ] Purge removes database, image files, and application state
- [ ] Runtime/glibc baseline is recorded
- [ ] README installation instructions match the released behavior

A release should only be described as supported on environments that have actually passed the relevant validation.