# NaviFS (Navigation File System)

> **The Universal Plug & Play Filesystem Intelligence Engine for AI Agents & IDEs**
> *Sub-10ms Hybrid Search (SQLite FTS5 + BM25) • Knowledge Graph • Model Context Protocol (MCP)*

NaviFS transforms local codebases into high-fidelity, ranked context for autonomous AI agents. Powered by a single high-performance Rust engine, it eliminates fragmented file searches and delivers instant lexical/semantic search, bounded content extraction, and relationship mapping through the standard **Model Context Protocol (MCP)**.

---

## ⚡ 1-Click Plug & Play Setup

No manual JSON editing. NaviFS automatically detects and configures your installed AI agents (**Claude Desktop**, **Cursor IDE**, **Antigravity IDE**, **Windsurf**, and **VS Code / Cline**).

### Windows (PowerShell)
```powershell
.\install.ps1
```
*Or non-interactive automated setup:*
```powershell
.\install.ps1 -All
```

### macOS & Linux (Bash / Zsh)
```bash
./install.sh
```

---

## 🎯 Supported AI Agents & IDEs

NaviFS works natively out-of-the-box with any MCP-compatible AI agent or editor:

| AI Client | Setup Support | Config Location |
| :--- | :---: | :--- |
| **Claude Desktop** | Auto-Configured | `claude_desktop_config.json` |
| **Cursor IDE** | Auto-Configured | `.cursor/mcp.json` |
| **Antigravity IDE** | Auto-Configured | `.agents/mcp_config.json` & global config |
| **Windsurf (Cascade)** | Auto-Configured | `~/.codeium/windsurf/mcp_config.json` |
| **VS Code (Cline / Roo)**| Auto-Configured | `cline_mcp_settings.json` |
| **Any Custom Agent** | Universal Stdio | `"command": "navifs", "args": ["mcp"]` |

### Manual MCP Configuration (Any Client)
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

## 🛠️ CLI Commands

Once installed, the `navifs` binary is available globally in your PATH:

```powershell
# Interactive auto-configurator for your AI clients
navifs setup

# Launch the MCP server over standard I/O (used by AI IDEs)
navifs mcp

# Search indexed files directly from your terminal
navifs search "migration logic" --limit 5

# Recursively index any folder or workspace
navifs index "C:\path\to\project"

# Run as a background daemon with live filesystem watching
navifs daemon --watch "C:\path\to\project"
```

---

## 🧩 Architecture

NaviFS is architected as a modular, local-first engine with zero cloud dependencies:

```text
NaviFS/
├── Cargo.toml                  # Root multi-crate workspace definition
├── install.ps1                 # Windows 1-click installer & auto-configurator
├── install.sh                  # macOS/Linux 1-click installer & auto-configurator
├── website/                    # MNC-grade developer documentation portal
├── crates/
│   ├── core/                   # Domain models, cryptographic hashes, interfaces
│   ├── database/               # SQLite FTS5 persistence (WAL mode, cascades, sync triggers)
│   ├── extract/                # Multi-modal parsers (PDF, XLSX, DOCX, Markdown, Code)
│   ├── search/                 # 2-stage hybrid retrieval (FTS5 + Path + RRF k=20 + Reranker)
│   ├── graph/                  # Bidirectional property graph and BFS traversal
│   ├── indexer/                # Directory scanner and debounced filesystem watcher
│   ├── mcp/                    # Model Context Protocol (MCP 2024-11-05) JSON-RPC 2.0 stdio server
│   └── daemon/                 # CLI binary & auto-setup engine (`navifs`)
```

---

## 🚀 Performance Highlights

- **Sub-10ms Search Latency**: In-memory cached SQLite FTS5 index executes queries in ~8ms.
- **Rank-Fused Relevance**: Blends BM25 lexical score (60%) with path hierarchy (40%) and freshness.
- **Bounded Content Windows**: Extracts precise line ranges with evidence locators to prevent LLM context explosion.
- **Zero Configuration Drift**: A single source of truth across all your AI development tools.

---

## 📖 Developer Documentation & Showcase

Open [`website/index.html`](website/index.html) in any browser to explore the interactive MCP tool playground, architectural specifications, and live demonstrations.
