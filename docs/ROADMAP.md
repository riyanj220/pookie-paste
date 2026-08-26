# Pookie Paste Roadmap


## Phase 0 — Foundation

Goal:

Create the project foundation.

Completed:

- Rust workspace
- Project structure
- Development tooling
- Documentation
- CI pipeline


## Phase 1 — Daemon Foundation

Goal:

Create the background clipboard service.

Tasks:

- Daemon lifecycle
- Async runtime
- Logging
- Configuration
- Graceful shutdown


## Phase 2 — Clipboard Engine

Goal:

Capture clipboard changes.

Tasks:

- X11 clipboard support
- Wayland clipboard support
- Clipboard abstraction layer


## Phase 3 — Clipboard Processing

Goal:

Process clipboard data.

Tasks:

- Content detection
- Hash generation
- Duplicate filtering
- Normalization


## Phase 4 — Storage Layer

Goal:

Persist clipboard history.

Tasks:

- SQLite integration
- Repository pattern
- Database migrations


## Phase 5 — IPC Layer

Goal:

Connect UI and daemon.

Tasks:

- IPC protocol
- Request handling
- Event communication


## Phase 6 — User Interface

Goal:

Build the clipboard history experience.

Features:

- History popup
- Keyboard navigation
- Clipboard selection


## Phase 7 — Windows Clipboard Features

Goal:

Match Windows Clipboard History experience.

Features:

- Pin items
- Recent history
- Quick retrieval


## Phase 8 — Search

Goal:

Fast clipboard discovery.

Features:

- Full-text search
- Filtering
- Ranking


## Phase 9 — Optimization

Goal:

Improve performance.

Tasks:

- Memory optimization
- Startup optimization
- Benchmarking


## Phase 10 — Security

Goal:

Improve reliability and privacy.

Tasks:

- Data protection
- Permission handling
- Secure storage


## Phase 11 — Linux Integration

Goal:

Better desktop integration.

Support:

- GNOME
- KDE
- System startup
- Notifications


## Phase 12 — Release

Goal:

Prepare production release.

Tasks:

- Packaging
- Documentation
- Distribution
- Community guidelines
