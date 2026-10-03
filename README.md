# NaviFS (Navigation File System)

> **Local-First File System Intelligent Engine Exposed via Model Context Protocol (MCP)**

NaviFS is a high-performance, embedded, local-first engine designed to transform your file system into an intelligent knowledge base for LLMs. It monitors directories in real time, extracts content structure and symbols, indexes files for instant lexical and semantic search, builds a contextual knowledge graph, and exposes everything through the **Model Context Protocol (MCP)**.

---

## 🏛️ Architecture & Workspace Crates

```text
NaviFS/
├── Cargo.toml                  # Root multi-crate workspace definition
├── crates/
│   ├── core/                   # Domain models, identity structs, cryptographic hashes, traits
│   ├── database/               # Local-first SQLite persistence (files, chunks, entities, edges)
│   ├── extract/                # Multi-modal parsers (Markdown AST, code symbols, text windowing)
│   ├── search/                 # Lexical inverted-index & BM25 ranking engine
│   ├── graph/                  # In-memory knowledge graph and entity relationship traversal
│   ├── indexer/                # Directory scanner and indexing pipeline orchestrator
│   ├── mcp/                    # Model Context Protocol (MCP) JSON-RPC 2.0 server (stdio)
│   └── daemon/                 # CLI binary and background daemon (`navifs`)
```

---

## 🔑 Core Identity Primitives (`navifs-core`)

All domain types are strongly-typed, serialization-ready, and thread-safe:
- **`FileIdentity` & `FileId`**: Tracks file fingerprints, timestamps, MIME categorization, NTFS file index, and SHA-256 content hashes.
- **`FileChunk` & `ChunkId`**: Slices documents into indexable chunks with byte/line offsets and token estimations.
- **`EntityNode` & `EntityId`**: Represents structural entities (functions, structs, headings, files).
- **`RelationEdge` & `RelationId`**: Directed, weighted relationships (`Contains`, `Defines`, `References`, `Imports`).

---

## 🚀 Usage

### 1. Build the Workspace
```powershell
cargo build --release
```

### 2. Index a Directory
```powershell
cargo run -p navifs-daemon -- index "C:\path\to\your\project"
```

### 3. Search Indexed Knowledge
```powershell
cargo run -p navifs-daemon -- search "authentication middleware" --limit 5
```

### 4. Run the MCP Server for LLMs (Claude Desktop, Cursor, Antigravity)
```powershell
cargo run -p navifs-daemon -- mcp
```

#### Claude Desktop MCP Configuration:
```json
{
  "mcpServers": {
    "navifs": {
      "command": "C:\\path\\to\\NaviFS\\target\\release\\navifs.exe",
      "args": ["mcp"]
    }
  }
}
```
