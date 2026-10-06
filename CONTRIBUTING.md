# Contributing to NaviFS

Thank you for your interest in contributing to NaviFS!

NaviFS is a local-first filesystem intelligence engine designed for autonomous AI agents and IDEs, exposing high-performance hybrid search (SQLite FTS5 + BGE semantic embeddings via Candle) through the Model Context Protocol (MCP).

---

## 🛠️ Development Setup

### Prerequisites
- **Rust Toolchain**: `stable` (1.80+)
- **C Compiler**:
  - Windows: MSVC Build Tools (Visual Studio 2022 Community or Build Tools)
  - Linux: `build-essential` (`gcc`)
  - macOS: Xcode Command Line Tools (`clang`)

### Building from Source
```bash
# Clone the repository
git clone https://github.com/Dhruv6190/NaviFS.git
cd navifs

# Run full compilation check
cargo check --workspace --all-targets

# Run test suite
cargo test --workspace

# Build optimized release binary
cargo build --release -p navifs-daemon
```

The resulting binary will be located at `target/release/navifs` (or `navifs.exe` on Windows).

---

## 🧪 Testing Guidelines

1. **Unit & Integration Tests**:
   Ensure all existing and new unit tests pass:
   ```bash
   cargo test --workspace
   ```

2. **Code Formatting & Linting**:
   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   ```

---

## 📐 Project Structure

- `crates/core`: Domain models, identities, cryptographic content hashes, and engine interfaces.
- `crates/database`: SQLite persistence, migrations, WAL configuration, and FTS5 indexing.
- `crates/extract`: Multi-format parsers (code, Markdown, PDF, DOCX, XLSX, JSON, plain text).
- `crates/embed`: Local sentence embeddings using BGE-small running offline on pure-Rust Candle.
- `crates/search`: 2-stage hybrid search, Reciprocal Rank Fusion (RRF), and feature reranking.
- `crates/graph`: In-memory property graph and relationship traversal.
- `crates/indexer`: Recursive directory scanner, debounced filesystem watcher, and extraction pipeline.
- `crates/mcp`: Standard Model Context Protocol (MCP 2024-11-05) JSON-RPC 2.0 stdio server.
- `crates/daemon`: Command-line interface (`navifs`), daemon loop, and automated agent configurator.
- `eval`: Automated evaluation and validation harness for MCP tools.

---

## 📜 License

NaviFS is dual-licensed under either:
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))
- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
