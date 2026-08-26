# Pookie Paste Architecture

## Overview

Pookie Paste is a lightweight and fast clipboard history manager for Linux.

The project aims to provide a clipboard experience similar to Windows Clipboard History while maintaining a native Linux approach with minimal resource usage.

The primary goals are:

- Fast clipboard retrieval
- Low memory usage
- Reliable background operation
- Clean architecture
- Easy contribution for open-source developers

# High-Level Architecture

```text
+----------------+
|      UI        |
| Desktop Client |
+-------+--------+
        |
        |
        v
+----------------+
|      IPC       |
| Communication  |
| Layer          |
+-------+--------+
        |
        |
        v
+----------------+
|     Daemon     |
|   Background   |
|    Clipboard   |
|     Service    |
+-------+--------+
        |
        |
        v
+----------------+
|  Pookie-Core   |
| Business Logic |
+-------+--------+
        |
        |
        v
+----------------+
|   Storage      |
| SQLite Layer   |
+----------------+
```

# Component Responsibilities

## Daemon

The daemon is the background service responsible for running continuously.

Responsibilities:

- Monitor clipboard changes
- Capture clipboard events
- Coordinate internal services
- Manage application lifecycle
- Handle background execution

The daemon should remain lightweight and consume minimal system resources.

## Pookie-Core

The pookie-core crate contains the main application logic.

Responsibilities:

- Clipboard processing
- Duplicate detection
- Content validation
- Data transformation
- Business rules

The pookie-core layer should remain independent from:

- UI
- Database implementation
- Operating system APIs

## Storage

The storage layer handles persistent clipboard history.

Initial technology:

- SQLite

Responsibilities:

- Save clipboard entries
- Retrieve history
- Search stored data
- Manage database operations

The storage layer should expose clean interfaces instead of leaking database details.

## IPC

The IPC layer provides communication between the daemon and user interface.

The UI should never directly interact with clipboard monitoring logic.

Communication flow:

```text
UI Request

↓

IPC Layer

↓

Daemon

↓

Core Processing

↓

Storage

↓

Response
```

Responsibilities:

- Request handling
- Event communication
- Data serialization
- Process communication

## UI

The UI provides the user-facing clipboard experience.

Responsibilities:

- Display clipboard history
- Search clipboard items
- Keyboard navigation
- Select clipboard entries
- Trigger paste actions

# Design Principles

## Separation of Concerns

Each component should have a single clear responsibility.

The daemon should not contain UI logic.

The UI should not contain storage logic.

The storage layer should not contain clipboard logic.

## Performance First

Pookie Paste is designed around performance.

Important goals:

- Fast startup
- Instant clipboard access
- Minimal background resource usage
- Efficient memory usage

## Extensibility

The architecture should allow future features:

- Image clipboard support
- Rich text support
- HTML clipboard support
- Multiple desktop environments
- Additional operating systems

# Technology Choices

## Programming Language

Rust is used because it provides:

- Memory safety
- High performance
- Strong concurrency support
- Modern tooling

## Storage

Initial storage:

SQLite

Reason:

- Lightweight
- Embedded
- Reliable
- No external database required

# Future Platform Support

Initial target:

Linux

Potential future support:

- Windows
- macOS
