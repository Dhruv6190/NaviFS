# NaviFS (Navigation File System)

<div align="center">

[![CI](https://github.com/Dhruv6190/NaviFS/actions/workflows/ci.yml/badge.svg)](https://github.com/Dhruv6190/NaviFS/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Dhruv6190/NaviFS?color=blue&label=release)](https://github.com/Dhruv6190/NaviFS/releases)
[![License: MIT / Apache 2.0](https://img.shields.io/badge/License-MIT%20%2F%20Apache--2.0-green.svg)](LICENSE-MIT)
[![Rust: 1.80+](https://img.shields.io/badge/Rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![MCP: 2024-11-05](https://img.shields.io/badge/MCP-2024--11--05-blueviolet.svg)](https://modelcontextprotocol.io)
[![Embeddings: Candle Pure-Rust](https://img.shields.io/badge/Embeddings-Candle%20(Pure%20Rust)-purple.svg)](https://github.com/huggingface/candle)

**The Universal Plug & Play Filesystem Intelligence Engine for AI Agents & IDEs**  
*Sub-10ms 3-Channel Hybrid Retrieval (SQLite FTS5 + BGE Semantic Embeddings via Candle) • Bounded Range Extraction • Knowledge Graph • Model Context Protocol (MCP)*

[1-Click Setup](#-1-click-plug--play-setup) •
[Supported Clients](#-supported-ai-agents--ides) •
[Retrieval Architecture](#-3-channel-hybrid-retrieval) •
[MCP Tools Reference](#-model-context-protocol-mcp-tools) •
[CLI Commands](#%EF%B8%8F-cli-commands) •
[Crates Architecture](#-crates-architecture) •
[Benchmarks](#-benchmarks--performance)

</div>

---

## ⚡ The Problem & The NaviFS Solution

Autonomous AI coding agents (Claude Desktop, Cursor, Antigravity, Windsurf, VS Code / Cline) often stumble when working in non-trivial codebases:
1. **Context Window Exhaustion**: Sequential `cat` or whole-file reading floods the LLM with thousands of irrelevant tokens, driving up latency and inference costs.
2. **Hallucinated Files & Boundaries**: Agents guess file locations, method signatures, and relationships when they lack index-backed ground truth.
3. **Fragile Dependencies**: Many retrieval tools require complex external Python environments, heavy Docker containers, or fragile native C++ DLLs that fail on standard developer setups.

**NaviFS** is a standalone, single-binary, local-first intelligence engine written in 100% pure Rust:
- **3-Channel Hybrid Retrieval**: Parallel candidate generation across SQLite FTS5 lexical matching, path hierarchy scoping, and dense vector semantic cosine similarity.
- **Reciprocal Rank Fusion ($k=20$) & 4D Reranker**: Cross-channel confirmation combines lexical precision with semantic understanding.
- **Bounded Content Windows**: Agents retrieve exact line slices, page intervals, or byte ranges with deterministic resource URIs (`navifs://file/<uuid>/lines/10-40`).
- **Offline Pure-Rust AI**: Sentence embeddings run natively on CPU via Hugging Face `candle` (`BAAI/bge-small-en-v1.5`, 384 dimensions) with zero Python runtimes or external C++ DLL dependencies.
- **Zero-Drift Auto-Setup**: Detects and configures all installed AI editors in one step with atomic file updates and automatic backups.

---

## 🚀 1-Click Plug & Play Setup

NaviFS automatically detects your installed AI environments and configures their MCP client settings out-of-the-box.

### Windows (PowerShell)
```powershell
# 1-Line Remote Installer (downloads release, validates SHA-256, adds to PATH, and configures IDEs):
powershell -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/Dhruv6190/NaviFS/main/install.ps1 | iex"

# Or from a local clone:
.\install.ps1 -All
```

### macOS & Linux (Bash / Zsh)
```bash
# 1-Line Remote Installer:
curl -fsSL https://raw.githubusercontent.com/Dhruv6190/NaviFS/main/install.sh | bash

# Or from a local clone:
./install.sh
```

### Build or Install via Cargo
```bash
# Install directly from Git:
cargo install --git https://github.com/Dhruv6190/NaviFS.git navifs-daemon

# Run automated configuration for all detected AI agents:
navifs setup --all
```

---

## 🎯 Supported AI Agents & IDEs

NaviFS connects out-of-the-box with any MCP-compliant agent host:

| AI Host / Editor | Auto-Configured? | Standard Configuration Location |
| :--- | :---: | :--- |
| **Claude Desktop** | ✅ Yes (`navifs setup`) | `%APPDATA%\Claude\claude_desktop_config.json` (Windows)<br>`~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) |
| **Cursor IDE** | ✅ Yes (`navifs setup`) | `~/.cursor/mcp.json` |
| **Antigravity IDE** | ✅ Yes (`navifs setup`) | `~/.gemini/config/mcp_config.json` or `.agents/mcp_config.json` |
| **Windsurf (Cascade)** | ✅ Yes (`navifs setup`) | `~/.codeium/windsurf/mcp_config.json` |
| **VS Code (Cline / Roo)**| ✅ Yes (`navifs setup`) | `cline_mcp_settings.json` / `roo_code_mcp_settings.json` |
| **Custom Agent / CLI** | ✅ Universal Stdio | Spawns `navifs mcp` over JSON-RPC 2.0 stdio |

### Manual MCP Host Configuration (Any Client)
```json
{
  "mcpServers": {
    "navifs": {
      "command": "navifs",
      "args": ["mcp"]
    }
  }
}
```

---

## 🧠 3-Channel Hybrid Retrieval

NaviFS implements a balanced two-stage retrieval pipeline engineered for speed and precision:

```text
                                 [ User Query ]
                                       │
        ┌──────────────────────────────┼──────────────────────────────┐
        ▼                              ▼                              ▼
  [ Channel A ]                  [ Channel B ]                  [ Channel C ]
SQLite FTS5 Lexical            Path & Directory            Dense Semantic Vector
     (BM25)                      Hierarchy               (Candle BGE-small-en-v1.5)
        │                              │                              │
        └──────────────────────────────┼──────────────────────────────┘
                                       │
                                       ▼
                       [ Reciprocal Rank Fusion (RRF) ]
                       RRF(d) = Σ [ w_c / (20.0 + r_c(d)) ]
                                       │
                                       ▼
                           [ 4D Feature Reranker ]
              Score = 0.45·Lexical + 0.25·Path + 0.15·Freshness + 0.15·Quality
                                       │
                                       ▼
                    [ Top-K Candidates with Evidence Locators ]
```

### 1. Channel A: Full-Text Lexical (SQLite FTS5)
- Tokenized chunk search with automated SQLite database triggers.
- Content modifications, additions, and deletions update the FTS5 virtual table instantaneously with zero application sync delay.

### 2. Channel B: Scoped Path & Hierarchy Matching
- Tokenizes directory separators, snake_case, and camelCase identifiers.
- Favors target modules and relevant directories directly from the query context.

### 3. Channel C: Dense Vector Cosine Similarity (Pure-Rust Candle)
- Employs `BAAI/bge-small-en-v1.5` (384-dimensional dense vectors).
- Generates normalized embeddings locally on the CPU via pure-Rust `candle` without external DLL or Python overhead.
- Measures similarity using L2-normalized cosine distance.

### Mathematical Fusion & Reranking
1. **Reciprocal Rank Fusion (RRF)**:
   $$RRF(d) = \sum_{c \in \{lex, path, vec\}} \frac{w_c}{k + r_c(d)}$$
   where $k = 20.0$, $w_{lex} = 1.0$, $w_{path} = 0.7$, and $w_{vec} = 0.9$.
2. **4D Composite Reranking**:
   $$Score = (0.45 \times Lexical) + (0.25 \times Path) + (0.15 \times Freshness) + (0.15 \times Quality)$$

---

## 🛠️ CLI Commands

Once installed, the `navifs` binary is available globally:

```powershell
# Auto-configure your AI clients with atomic safety:
navifs setup
navifs setup --all
navifs setup --dry-run
navifs setup --uninstall
navifs setup --client cursor

# Launch the Model Context Protocol stdio server:
navifs mcp

# Execute 3-channel hybrid search directly from terminal:
navifs search "migration logic" --limit 5

# Recursively index any repository or directory:
navifs index "C:\path\to\project"

# Launch background daemon with debounced file watching:
navifs daemon --watch "C:\path\to\project"
```

---

## 🔌 Model Context Protocol (MCP) Tools

NaviFS exposes 4 deterministic tools adhering strictly to the **MCP 2024-11-05** specification:

### 1. `search`
Hybrid retrieval across FTS5, path matching, and semantic vectors.
- **Arguments**:
  - `query` (string, required): Natural language query or code identifiers.
  - `filters` (object, optional): `path_prefix`, `mime_types`, `modified_after`, `modified_before`.
  - `limit` (integer, optional): Maximum top-K candidates (default: 10).
- **Output**: Ranked candidate items with `score`, `features` (lexical, path, vector, freshness, quality), and exact `evidence` locators.

### 2. `open`
Bounded content range retrieval preventing context window explosions.
- **Arguments**:
  - `file_id` (UUID, optional*) or `path` (string, optional*): Target file locator.
  - `start_line` / `end_line` (integer, optional): 1-indexed line intervals.
  - `start_page` / `end_page` (integer, optional): 1-indexed PDF page bounds.
  - `byte_start` / `byte_end` (integer, optional): Raw byte boundaries.
  - `chunk_index` (integer, optional): Specific chunk index.
- **Output**: Structured content slice with `resource_uri` (e.g. `navifs://file/<id>/lines/1-25`), `locator_summary`, and included chunk identifiers.

### 3. `inspect`
Structural metadata, MIME classification, outline taxonomy, and temporal audit events.
- **Spreadsheets (`.xlsx`, `.xls`, `.ods`)**: Enumerates all worksheet names and dimensions.
- **Documents (`.docx`, `.md`)**: Discovers heading hierarchies (H1–H6).
- **Source Code**: Maps AST structs, traits, functions, and interfaces.
- **Audit Log**: Returns recent temporal events (`created`, `modified`, `indexed`, `deleted`).

### 4. `related`
One-hop knowledge graph traversal over directional property relations.
- **Relations**: `Contains` (parent to child), `References` (cites path), `Imports` (module dependency), `Defines` (type/schema declaration).
- **Output**: Incoming and outgoing edges with relation weights and destination entities.

---

## 📄 Multi-Modal Document Extraction

NaviFS processes codebases, enterprise spreadsheets, documents, and technical PDFs without external servers or Python runtime overhead:

| Format | Rust Engine | Chunking & Extraction Strategy |
| :--- | :--- | :--- |
| **Code & Text** (`.rs`, `.py`, `.ts`, `.js`, `.go`, `.c`, etc.) | Token Windowing | Sliding windows (512 tokens, 64-token overlap) with 1-indexed line attribution. |
| **Spreadsheets** (`.xlsx`, `.xls`, `.ods`) | `calamine` | Worksheets extracted into Markdown tables in 50-row chunks with cell coordinates. |
| **Word Documents** (`.docx`) | `quick-xml` + `zip` | Heading hierarchy tree (H1–H6), tabular data, and section chunks. |
| **PDF Documents** (`.pdf`) | Streaming `FlateDecode` | Stream operator parser (`Tj`/`TJ`), font mapping, and page-bound text chunks. |
| **Markdown** (`.md`) | AST Heading Lexer | Heading boundaries and semantic section blocks. |
| **Structured Data** (`.json`, `.csv`) | `serde_json` | Recursive key-path flattening and record boundaries. |

---

## 🧩 Crates Architecture

NaviFS is structured as a clean, highly modular Rust workspace of 9 specialized crates:

```text
NaviFS/
├── Cargo.toml                  # Workspace manifest with unified dependencies & release profile
├── install.ps1                 # Windows 1-click installer with SHA-256 verification
├── install.sh                  # macOS/Linux 1-click installer with SHA-256 verification
├── website/                    # Apple-style developer showcase & documentation SPA
├── crates/
│   ├── core/                   # Domain models (FileId, ChunkId, EvidenceLocator, MimeType, Traits)
│   ├── database/               # Embedded SQLite, WAL mode, foreign key cascades, FTS5 sync triggers
│   ├── extract/                # Multi-modal parsers (PDF, XLSX, DOCX, Markdown, Code, JSON)
│   ├── embed/                  # Pure-Rust offline embeddings (BGE-small via Hugging Face Candle)
│   ├── search/                 # 3-channel retrieval (Content FTS5 + Path + Vector + RRF + Reranker)
│   ├── graph/                  # Bidirectional property graph and BFS traversal
│   ├── indexer/                # Recursive directory scanner (WalkDir), SHA-256 detector, notify watcher
│   ├── mcp/                    # Model Context Protocol (MCP 2024-11-05) JSON-RPC 2.0 stdio server
│   └── daemon/                 # CLI entrypoint (`navifs`), daemon watcher, and auto-setup engine
└── eval/                       # Golden evaluation harness and benchmark validation suite
```

---

## 📊 Benchmarks & Performance

Measured on modern commodity developer hardware (x86_64, NVMe SSD):

| Metric | NaviFS Performance | Industry Standard / Naive Context |
| :--- | :---: | :---: |
| **FTS5 Lexical Search (P99)** | **&lt; 3 ms** | ~45–120 ms (External Search Engines) |
| **End-to-End Hybrid Search (P99)** | **&lt; 10 ms** | ~250–800 ms (Multi-Service APIs) |
| **Binary Size (Optimized)** | **~12.2 MB** | 150–500 MB (Electron / Python Runtimes) |
| **Context Window Reduction** | **94% Savings** | 0% (Full-file dumping) |
| **Background Idle CPU** | **0.0%** | Variable |
| **Automated Test Suite** | **30 / 30 Passed** | Codebase verified |

---

## 🤝 Contributing

Contributions are welcome! Please review [CONTRIBUTING.md](CONTRIBUTING.md) for complete guidelines.

### Local Development Quickstart
```bash
# Clone the repository:
git clone https://github.com/Dhruv6190/NaviFS.git
cd NaviFS

# Run the test suite:
cargo test --workspace

# Run evaluation benchmarks:
cargo run -p navifs-eval --release

# Format code:
cargo fmt --check
```

---

## 📜 License

NaviFS is dual-licensed under:
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))
- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

You may choose either license at your option.
