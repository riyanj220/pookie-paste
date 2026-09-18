# Pookie Paste Roadmap

Pookie Paste is actively evolving. This roadmap highlights planned areas of work rather than fixed implementation phases.

## Current

### Clipboard History Actions

Planned improvements to the history experience:

- Delete individual history items
- Clear clipboard history
- Pin or favorite items
- Small context-menu actions
- Related history utilities

## Next

### Reliability and UX Hardening

- Continue X11 and Wayland edge-case testing
- Improve popup and focus behavior
- Improve handling of transient desktop surfaces
- Performance and memory profiling
- Additional regression coverage

### Desktop Support

Current primary targets:

- X11
- KDE Plasma Wayland

Future work:

- GNOME Wayland
- Additional Wayland compositors where practical

## Future Content Types

The current clipboard model supports:

- Text
- Images

Potential future additions:

- File clipboard entries
- HTML / rich text
- Additional clipboard formats

These will be added only when they fit cleanly into the existing clipboard-content and persistence architecture.

## Distribution

Future distribution work may include:

- `.deb` packages
- `.rpm` packages
- Arch / AUR packaging
- Additional release automation

## Longer-Term Ideas

Possible future improvements:

- Better search and filtering
- History organization
- Optional settings/preferences UI
- Additional desktop integrations