# Development Guide

## Requirements

Install:

- Rust
- Cargo
- Git

## Installing Rust

Install Rust using rustup:

```bash
curl --proto '=https' --tlsv1.3 https://sh.rustup.rs -sSf | sh
```

Verify installation:

```bash
rustc --version

cargo --version
```

## Clone Repository

```bash
git clone https://github.com/riyanj220/pookie-paste.git

cd pookie-paste
```

## Build Project

```bash
cargo build
```

## Run Checks

### Format

```bash
cargo fmt --check
```

### Lint

```bash
cargo clippy --all-targets --all-features
```

### Tests

```bash
cargo test
```

# Development Workflow

1. Create a feature branch.

Example:

```bash
git checkout -b feature/clipboard-engine
```

2. Implement changes.

3. Add tests.

4. Run validation:

```bash
cargo fmt
cargo clippy
cargo test
```

5. Create a pull request.

# Project Structure

```text
crates/

├── daemon
├── ui
├── pookie-core
├── storage
└── ipc
```

Each crate has a specific responsibility.

Avoid putting unrelated logic into other crates.

The goal is to keep the codebase modular, maintainable, and easy for contributors to understand.
