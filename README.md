# NaviFS (Navigation File System)

> **Deterministic Filesystem Intelligence Engine Exposed via Model Context Protocol (MCP)**

NaviFS is a high-performance, embedded, local-first engine designed to transform physical filesystems into high-fidelity context for autonomous AI agents and LLMs. It monitors directories in real time, extracts content structure and symbols, indexes files for instant lexical and semantic search, builds a contextual knowledge graph, and exposes everything through the standard **Model Context Protocol (MCP)**.

- **Official Website & Apple-Style Developer Documentation**: Located in [`website/index.html`](website/index.html).

---

## Architecture & Workspace Crates

```text
NaviFS/
├── Cargo.toml                  # Root multi-crate workspace definition
├── website/                    # MNC-grade landing page and Apple-style developer docs portal
├── eval/                       # Benchmark evaluation suite & automated MCP trace harness
├── crates/
│   ├── core/                   # Domain models, identity structs, cryptographic hashes, interfaces
│   ├── database/               # Local-first SQLite persistence (WAL mode, cascades, FTS5 sync triggers)
│   ├── extract/                # Multi-modal parsers (PDF, XLSX calamine, DOCX quick-xml, Markdown, JSON)
│   ├── search/                 # 2-stage hybrid retrieval (FTS5 + Path + Vector ANN + RRF k=20 + Reranker)
│   ├── graph/                  # In-memory bidirectional property graph and BFS traversal
│   ├── indexer/                # Directory scanner, notify debounced watcher, and temporal event logger
│   ├── mcp/                    # Model Context Protocol (MCP 2024-11-05) JSON-RPC 2.0 stdio server
│   └── daemon/                 # CLI binary and background daemon (`navifs`)
```

---

## Core Identity Primitives (`navifs-core`)

All domain types are strongly-typed, serialization-ready, and thread-safe:
- **`FileIdentity` & `FileId`**: Tracks file fingerprints, timestamps, MIME categorization, NTFS file index, and SHA-256 content hashes.
- **`FileChunk` & `ChunkId`**: Slices documents into indexable chunks with byte/line offsets and token estimations.
- **`EntityNode` & `EntityId`**: Represents structural entities (functions, structs, headings, worksheets).
- **`RelationEdge` & `RelationId`**: Directed, weighted relationships (`Contains`, `Defines`, `References`, `Imports`).
- **`FileTemporalEvent`**: Persistent audit records tracking file mutations (`created`, `modified`, `indexed`, `deleted`).

---

## Usage

### 1. Build the Workspace
```powershell
cargo build --release
```

### 2. Run Tests
```powershell
cargo test --workspace
```

### 3. Run the MCP Server for AI Agents (Claude Desktop, Cursor, Antigravity)
```powershell
cargo run -p navifs-daemon -- mcp --path "C:\path\to\your\project"
```

#### Claude Desktop Configuration (`claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "navifs": {
      "command": "C:\\path\\to\\NaviFS\\target\\release\\navifs.exe",
      "args": ["mcp", "--path", "C:\\path\\to\\your\\project"]
    }
  }
}
```

#### Cursor IDE Configuration:
- Settings > Features > MCP Servers > Add New Server
- Type: `command`
- Command: `C:\path\to\NaviFS\target\release\navifs.exe mcp --path .`

---

## Documentation & Web Portal

Open [`website/index.html`](website/index.html) in any modern browser to view the MNC product showcase, interactive MCP tool playground, and Apple-style developer documentation portal.
