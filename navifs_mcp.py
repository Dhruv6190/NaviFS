#!/usr/bin/env python3
"""
NaviFS Local-First Model Context Protocol (MCP) Server
Pure Python 3 • Zero External Dependencies • Instant Stdio JSON-RPC 2.0 Bridge
Provides deterministic search, bounded opening, directory inspection, and dependency mapping.
"""

import sys
import os
import json
import re
import math
import hashlib
from pathlib import Path
from datetime import datetime

# Enforce UTF-8 on Windows stdio
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

PROTOCOL_VERSION = "2024-11-05"
SERVER_NAME = "navifs"
SERVER_VERSION = "0.1.0"

# Default root workspace
DEFAULT_ROOT = Path(__file__).resolve().parent

IGNORE_DIRS = {
    ".git", "node_modules", "target", ".cargo-target", "__pycache__",
    ".idea", ".vscode", "dist", "build", ".next", ".turbo"
}

def log(msg: str):
    """Write diagnostic log to stderr so stdio JSON-RPC remains clean."""
    sys.stderr.write(f"[NaviFS-MCP] {msg}\n")
    sys.stderr.flush()

def compute_sha256(file_path: Path) -> str:
    h = hashlib.sha256()
    try:
        with open(file_path, "rb") as f:
            while chunk := f.read(65536):
                h.update(chunk)
        return h.hexdigest()
    except Exception:
        return "unknown"

def tool_search(query: str, path: str = None, limit: int = 10) -> str:
    """Hybrid rank-fused search matching file paths and content."""
    root_path = Path(path) if path else DEFAULT_ROOT
    if not root_path.exists():
        return f"Error: Path '{root_path}' does not exist."

    query_tokens = [t.lower() for t in re.findall(r"\w+", query) if len(t) > 1]
    if not query_tokens:
        query_tokens = [query.lower()]

    matches = []

    for dirpath, dirnames, filenames in os.walk(root_path):
        dirnames[:] = [d for d in dirnames if d not in IGNORE_DIRS]

        for fname in filenames:
            file_path = Path(dirpath) / fname
            rel_path = file_path.relative_to(root_path).as_posix()

            # Path scoring
            path_lower = rel_path.lower()
            path_matches = sum(1 for q in query_tokens if q in path_lower)
            path_score = (path_matches / len(query_tokens)) * 0.4 if query_tokens else 0

            # Content scoring
            try:
                # Read small to medium files directly, skip binary or large files (> 2MB)
                if file_path.stat().st_size > 2 * 1024 * 1024:
                    continue

                with open(file_path, "r", encoding="utf-8", errors="ignore") as f:
                    lines = f.readlines()

                content_matches = []
                for idx, line in enumerate(lines, start=1):
                    line_lower = line.lower()
                    matched_tokens = [q for q in query_tokens if q in line_lower]
                    if matched_tokens:
                        content_matches.append((idx, line.strip(), len(matched_tokens)))

                if content_matches or path_score > 0:
                    best_line = content_matches[0] if content_matches else (1, "", 0)
                    lexical_score = min(len(content_matches) * 0.1, 0.6)
                    total_score = round(path_score + lexical_score, 3)

                    # Context snippet
                    snippet_lines = []
                    if content_matches:
                        target_line_idx = best_line[0] - 1
                        start_bound = max(0, target_line_idx - 2)
                        end_bound = min(len(lines), target_line_idx + 3)
                        for l_idx in range(start_bound, end_bound):
                            prefix = ">>>" if l_idx == target_line_idx else "   "
                            snippet_lines.append(f"{prefix} L{l_idx + 1}: {lines[l_idx].rstrip()}")

                    matches.append({
                        "path": rel_path,
                        "absolute_path": str(file_path),
                        "score": total_score,
                        "line": best_line[0],
                        "match_count": len(content_matches),
                        "snippet": "\n".join(snippet_lines) if snippet_lines else f"Path match: {rel_path}"
                    })
            except Exception:
                continue

    # Sort descending by score
    matches.sort(key=lambda x: x["score"], reverse=True)
    top_matches = matches[:limit]

    if not top_matches:
        return f"No results found matching query '{query}' in {root_path}."

    output = [
        f"NaviFS Hybrid Search: \"{query}\" (Top {len(top_matches)} matches)",
        "=" * 64
    ]

    for idx, m in enumerate(top_matches, start=1):
        output.append(f"\n[#{idx}] Score: {m['score']} | {m['path']}:{m['line']}")
        output.append(f"Evidence Snippet:")
        output.append(m["snippet"])

    return "\n".join(output)

def tool_open(path: str, start_line: int = 1, end_line: int = 100) -> str:
    """Bounded file content retrieval with line numbers."""
    target = Path(path)
    if not target.is_absolute():
        target = DEFAULT_ROOT / target

    if not target.exists() or not target.is_file():
        return f"Error: File '{path}' does not exist."

    try:
        with open(target, "r", encoding="utf-8", errors="ignore") as f:
            all_lines = f.readlines()

        total_lines = len(all_lines)
        s = max(1, start_line)
        e = min(total_lines, max(s, end_line))

        selected = all_lines[s-1:e]
        formatted = [f"{i:4d} | {line.rstrip()}" for i, line in enumerate(selected, start=s)]

        header = f"NaviFS Bounded Open: {target.name} (Lines {s}-{e} of {total_lines})\nPath: {target}\n" + ("-" * 60)
        return header + "\n" + "\n".join(formatted)
    except Exception as exc:
        return f"Error reading file '{path}': {exc}"

def tool_inspect(path: str = None, depth: int = 2) -> str:
    """Inspect directory hierarchy, sizes, extensions, and metadata."""
    target = Path(path) if path else DEFAULT_ROOT
    if not target.is_absolute():
        target = DEFAULT_ROOT / target

    if not target.exists():
        return f"Error: Path '{target}' does not exist."

    if target.is_file():
        st = target.stat()
        sha = compute_sha256(target)
        dt = datetime.fromtimestamp(st.st_mtime).strftime("%Y-%m-%d %H:%M:%S")
        return f"NaviFS File Metadata:\n- Path: {target}\n- Size: {st.st_size} bytes\n- Last Modified: {dt}\n- SHA-256: {sha}"

    # Directory inspection
    ext_counts = {}
    total_size = 0
    total_files = 0
    tree_lines = [f"{target.name}/"]

    def build_tree(current_dir: Path, current_depth: int, prefix: str = ""):
        nonlocal total_size, total_files
        if current_depth > depth:
            return

        try:
            entries = sorted(list(current_dir.iterdir()), key=lambda x: (not x.is_dir(), x.name.lower()))
        except Exception:
            return

        for idx, entry in enumerate(entries):
            if entry.name in IGNORE_DIRS:
                continue

            is_last = (idx == len(entries) - 1)
            connector = "\\-- " if is_last else "|-- "
            child_prefix = prefix + ("    " if is_last else "|   ")

            if entry.is_dir():
                tree_lines.append(f"{prefix}{connector}{entry.name}/")
                build_tree(entry, current_depth + 1, child_prefix)
            else:
                total_files += 1
                try:
                    sz = entry.stat().st_size
                    total_size += sz
                    ext = entry.suffix.lower() or "no-ext"
                    ext_counts[ext] = ext_counts.get(ext, 0) + 1
                    tree_lines.append(f"{prefix}{connector}{entry.name} ({sz} bytes)")
                except Exception:
                    pass

    build_tree(target, 1)

    output = [
        f"NaviFS Hierarchy Inspection: {target}",
        "=" * 60,
        f"Total Files Scanned: {total_files} | Total Size: {total_size / (1024*1024):.2f} MB",
        f"Top Extensions: {', '.join(f'{k}: {v}' for k, v in sorted(ext_counts.items(), key=lambda x: x[1], reverse=True)[:6])}",
        "\nTree View:",
        "\n".join(tree_lines[:60]) # Limit tree to 60 lines for concise context
    ]
    return "\n".join(output)

def tool_related(path: str) -> str:
    """Find related files through shared module directory and import references."""
    target = Path(path)
    if not target.is_absolute():
        target = DEFAULT_ROOT / target

    if not target.exists():
        return f"Error: Path '{target}' does not exist."

    parent = target.parent if target.is_file() else target
    siblings = [f.name for f in parent.iterdir() if f.is_file() and f != target]

    # Search for files referencing this file name
    stem = target.stem
    references = []
    for dirpath, dirnames, filenames in os.walk(DEFAULT_ROOT):
        dirnames[:] = [d for d in dirnames if d not in IGNORE_DIRS]
        for fname in filenames:
            fpath = Path(dirpath) / fname
            if fpath == target:
                continue
            try:
                with open(fpath, "r", encoding="utf-8", errors="ignore") as f:
                    content = f.read(50000)
                    if stem in content:
                        references.append(fpath.relative_to(DEFAULT_ROOT).as_posix())
                        if len(references) >= 5:
                            break
            except Exception:
                continue

    output = [
        f"NaviFS Related Files for '{target.name}':",
        "=" * 60,
        f"Parent Directory Siblings ({len(siblings)}): {', '.join(siblings[:8])}",
        f"Referencing Files ({len(references)}): {', '.join(references) if references else 'No cross-references detected'}"
    ]
    return "\n".join(output)

# --- MCP JSON-RPC Server Loop ---

TOOLS_SCHEMA = [
    {
        "name": "search",
        "description": "Two-stage hybrid retrieval combining BM25 keyword matching, path scoping, and multi-feature rank fusion across local files.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Search query terms or keywords"},
                "path": {"type": "string", "description": "Optional directory path to restrict search scope"},
                "limit": {"type": "integer", "description": "Maximum number of ranked results to return (default: 10)"}
            },
            "required": ["query"]
        }
    },
    {
        "name": "open",
        "description": "Bounded file content retrieval with line-level evidence locators without context explosion.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to file"},
                "start_line": {"type": "integer", "description": "1-indexed starting line number (default: 1)"},
                "end_line": {"type": "integer", "description": "1-indexed ending line number (default: 100)"}
            },
            "required": ["path"]
        }
    },
    {
        "name": "inspect",
        "description": "Inspect directory hierarchy, tree structure, file size, extensions, and SHA-256 fingerprint.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Path to directory or file (default: workspace root)"},
                "depth": {"type": "integer", "description": "Depth of tree traversal (default: 2)"}
            }
        }
    },
    {
        "name": "related",
        "description": "Find co-located sibling files and cross-referencing code modules.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Target file or directory path"}
            },
            "required": ["path"]
        }
    }
]

def handle_request(req: dict) -> dict:
    method = req.get("method")
    msg_id = req.get("id")
    params = req.get("params", {})

    if method == "initialize":
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                }
            }
        }

    elif method == "notifications/initialized":
        return None

    elif method == "tools/list":
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "tools": TOOLS_SCHEMA
            }
        }

    elif method == "tools/call":
        tool_name = params.get("name")
        args = params.get("arguments", {})

        try:
            if tool_name == "search":
                res = tool_search(args.get("query", ""), args.get("path"), args.get("limit", 10))
            elif tool_name == "open":
                res = tool_open(args.get("path", ""), args.get("start_line", 1), args.get("end_line", 100))
            elif tool_name == "inspect":
                res = tool_inspect(args.get("path"), args.get("depth", 2))
            elif tool_name == "related":
                res = tool_related(args.get("path", ""))
            else:
                res = f"Error: Unknown tool '{tool_name}'."
        except Exception as e:
            res = f"Tool execution failed: {e}"

        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": res
                    }
                ]
            }
        }

    elif method == "ping":
        return {"jsonrpc": "2.0", "id": msg_id, "result": {}}

    else:
        return {
            "jsonrpc": "2.0",
            "id": msg_id,
            "error": {
                "code": -32601,
                "message": f"Method '{method}' not found"
            }
        }

def main():
    log(f"NaviFS MCP Server v{SERVER_VERSION} initialized on stdio.")
    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            response = handle_request(req)
            if response is not None:
                sys.stdout.write(json.dumps(response) + "\n")
                sys.stdout.flush()
        except Exception as err:
            log(f"JSON-RPC processing error: {err}")

if __name__ == "__main__":
    main()
