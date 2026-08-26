# Contributing to Pookie Paste

Thank you for your interest in contributing to Pookie Paste.

Pookie Paste is an open-source clipboard history manager for Linux focused on speed, reliability, simplicity, and a polished user experience.

Contributions of all kinds are welcome, including:

- Bug fixes
- Performance improvements
- Documentation improvements
- Tests
- Platform compatibility fixes
- Accessibility improvements
- Feature proposals

## Before You Start

For significant changes, please open an issue first to discuss the proposed change.

This helps avoid duplicated work and makes sure the implementation fits the project's architecture and goals.

Small fixes such as documentation improvements, typo corrections, and straightforward bug fixes can usually be submitted directly.

## Development Setup

See the development guide:

[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)

Before submitting changes, make sure the project builds and all checks pass.

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
```

## Branch Naming

Use descriptive branch names.

Examples:

```text
feature/clipboard-monitoring
feature/search
fix/duplicate-detection
fix/wayland-crash
docs/architecture-update
refactor/ipc-protocol
```

Avoid vague branch names such as:

```text
changes
update
work
test
```

## Commit Messages

Write clear commit messages describing what changed.

Good examples:

```text
Add X11 clipboard monitoring
Add SQLite repository abstraction
Fix duplicate clipboard entry detection
Improve daemon shutdown handling
Update development documentation
```

Avoid vague commit messages such as:

```text
fix
changes
update stuff
working
final
```

Keep commits focused on one logical change whenever possible.

## Pull Requests

Before opening a pull request:

1. Make sure your branch is up to date.
2. Run formatting checks.
3. Run Clippy.
4. Run the test suite.
5. Add or update tests when behavior changes.
6. Update documentation when necessary.

A pull request should clearly explain:

- What changed
- Why the change was needed
- How the change was tested
- Any compatibility or performance considerations

Keep pull requests focused. Large unrelated changes should be split into separate pull requests.

## Coding Guidelines

Pookie Paste follows standard Rust engineering practices.

General expectations:

- Prefer clear code over clever code.
- Keep modules focused.
- Avoid unnecessary dependencies.
- Handle errors explicitly.
- Avoid panics in normal runtime paths.
- Write tests for important behavior.
- Keep platform-specific logic behind abstractions.
- Avoid unnecessary allocations in performance-sensitive code.
- Document non-obvious architectural decisions.

## Architecture

Before making architectural changes, read:

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)

The project is intentionally separated into:

```text
UI
 |
IPC
 |
Daemon
 |
Core
 |
Storage
```

Changes should preserve clear boundaries between these components.

## Performance

Performance is an important project goal.

Changes affecting hot paths should avoid unnecessary:

- Allocations
- Copies
- Blocking operations
- Database queries
- Background CPU usage
- Memory usage

When introducing a performance-sensitive change, include benchmarks or measurements when practical.

## Dependencies

Avoid adding dependencies unless they provide clear value.

Before adding a new dependency, consider:

- Whether the functionality can reasonably be implemented without it
- Maintenance status
- Security history
- Binary size impact
- Compile-time impact
- Runtime cost
- Linux compatibility

Dependencies should be added intentionally rather than for convenience.

## Testing

Tests should be added when introducing new behavior or fixing bugs.

The project will use multiple levels of testing as it grows:

- Unit tests
- Integration tests
- Platform-specific tests
- End-to-end tests
- Performance benchmarks

Bug fixes should ideally include a regression test.

## Documentation

Documentation should be updated when changes affect:

- Architecture
- Setup instructions
- User behavior
- Configuration
- Public APIs
- Development workflow

Keep documentation concise and accurate.

## Issues

When reporting a bug, include as much relevant information as possible:

- Linux distribution
- Desktop environment
- X11 or Wayland
- Pookie Paste version
- Steps to reproduce
- Expected behavior
- Actual behavior
- Logs or error messages when available

Do not include passwords, tokens, clipboard secrets, or other sensitive information in issue reports.

## Feature Requests

Feature requests are welcome.

Please explain:

- The problem the feature solves
- The expected user experience
- Why it belongs in Pookie Paste
- Any alternatives you have considered

Pookie Paste aims to remain lightweight and focused, so not every feature request will necessarily be accepted.

## Code of Conduct

All contributors must follow the project's:

[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)

## License

By contributing to Pookie Paste, you agree that your contributions will be licensed under the project's MIT License.
