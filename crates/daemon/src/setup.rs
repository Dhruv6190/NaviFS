//! Automated Plug & Play setup for NaviFS across all AI agents and IDEs.
//! Configures Claude Desktop, Cursor, Antigravity, Windsurf, and VS Code (Cline/Roo).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ClientTarget {
    pub id: &'static str,
    pub name: &'static str,
    pub config_paths: Vec<PathBuf>,
}

impl ClientTarget {
    pub fn is_detected(&self) -> bool {
        self.config_paths.iter().any(|p| {
            if p.exists() {
                true
            } else if let Some(parent) = p.parent() {
                parent.exists()
            } else {
                false
            }
        })
    }

    pub fn primary_config_path(&self) -> &PathBuf {
        // Return existing config path if found, or first candidate
        self.config_paths
            .iter()
            .find(|p| p.exists())
            .unwrap_or(&self.config_paths[0])
    }
}

pub fn get_supported_clients() -> Vec<ClientTarget> {
    let home = dirs_home().unwrap_or_else(|| PathBuf::from("."));
    let app_data = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| home.join("AppData/Roaming"));

    vec![
        ClientTarget {
            id: "claude",
            name: "Claude Desktop",
            config_paths: vec![
                // Windows
                app_data.join("Claude/claude_desktop_config.json"),
                // macOS
                home.join("Library/Application Support/Claude/claude_desktop_config.json"),
                // Linux
                home.join(".config/Claude/claude_desktop_config.json"),
            ],
        },
        ClientTarget {
            id: "cursor",
            name: "Cursor IDE",
            config_paths: vec![
                PathBuf::from(".cursor/mcp.json"),
                home.join(".cursor/mcp.json"),
                app_data.join("Cursor/User/mcp.json"),
            ],
        },
        ClientTarget {
            id: "antigravity",
            name: "Antigravity IDE",
            config_paths: vec![
                PathBuf::from(".agents/mcp_config.json"),
                home.join(".gemini/config/mcp_config.json"),
            ],
        },
        ClientTarget {
            id: "windsurf",
            name: "Windsurf (Cascade)",
            config_paths: vec![
                home.join(".codeium/windsurf/mcp_config.json"),
            ],
        },
        ClientTarget {
            id: "vscode",
            name: "VS Code (Cline / Roo Code)",
            config_paths: vec![
                app_data.join("Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json"),
                home.join(".config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json"),
                home.join("Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json"),
            ],
        },
    ]
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .ok()
}

pub fn run_setup(auto_all: bool, target_client: Option<String>) -> Result<()> {
    let exe_path = std::env::current_exe().context("Could not determine current executable path")?;
    let canonical_exe = exe_path.canonicalize().unwrap_or(exe_path);
    let mut exe_str = canonical_exe.to_string_lossy().to_string();
    if exe_str.starts_with(r"\\?\") {
        exe_str = exe_str[4..].to_string();
    }

    println!("\n========================================================");
    println!("        NaviFS Plug & Play Auto-Configurator            ");
    println!("========================================================");
    println!("Executable: {}\n", exe_str);

    let clients = get_supported_clients();
    let mut selected_indices: Vec<usize> = Vec::new();

    if let Some(target) = target_client {
        let t_lower = target.to_lowercase();
        if let Some(idx) = clients.iter().position(|c| c.id == t_lower || c.name.to_lowercase().contains(&t_lower)) {
            selected_indices.push(idx);
        } else {
            eprintln!("Unknown client '{}'. Supported: claude, cursor, antigravity, windsurf, vscode", target);
            return Ok(());
        }
    } else if auto_all {
        println!("Mode: Auto-configure all detected AI clients\n");
        for (i, c) in clients.iter().enumerate() {
            if c.is_detected() {
                selected_indices.push(i);
            }
        }
        if selected_indices.is_empty() {
            // If none detected, configure all by default
            selected_indices = (0..clients.len()).collect();
        }
    } else {
        // Interactive selection prompt
        println!("Select which AI Agents / IDEs to configure:\n");
        for (i, client) in clients.iter().enumerate() {
            let status = if client.is_detected() {
                "[Detected on system]"
            } else {
                "[Not yet installed]"
            };
            println!("  [{}] {:<28} {}", i + 1, client.name, status);
        }
        println!("  [A] All of the above (Recommended)");
        println!("  [Q] Quit without changes\n");

        print!("Enter selection (e.g. '1, 3' or 'A') [default: A]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();

        if trimmed.eq_ignore_ascii_case("q") {
            println!("Setup cancelled.");
            return Ok(());
        }

        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("a") {
            selected_indices = (0..clients.len()).collect();
        } else {
            for part in trimmed.split([',', ' ']) {
                if let Ok(num) = part.trim().parse::<usize>() {
                    if num >= 1 && num <= clients.len() {
                        selected_indices.push(num - 1);
                    }
                }
            }
        }
    }

    if selected_indices.is_empty() {
        println!("No clients selected.");
        return Ok(());
    }

    println!("\nApplying configurations...");
    for &idx in &selected_indices {
        let client = &clients[idx];
        let config_path = client.primary_config_path();

        match inject_navifs_config(config_path, &exe_str) {
            Ok(created) => {
                let action = if created { "Created & configured" } else { "Updated" };
                println!("  [OK] {:<25} -> {} ({})", client.name, config_path.display(), action);
            }
            Err(e) => {
                println!("  [!] {:<25} -> Skipped ({}): {}", client.name, config_path.display(), e);
            }
        }
    }

    println!("\n--------------------------------------------------------");
    println!("NaviFS is now active and ready in your selected AI tools!");
    println!("Restart your IDE / Agent to begin blazing-fast search.");
    println!("========================================================\n");

    Ok(())
}

fn inject_navifs_config(path: &Path, exe_path: &str) -> Result<bool> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {:?}", parent))?;
    }

    let mut created = false;
    let mut root: Value = if path.exists() {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read existing config at {:?}", path))?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        created = true;
        json!({})
    };

    if !root.is_object() {
        root = json!({});
    }

    let mcp_servers = root
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| json!({}));

    if !mcp_servers.is_object() {
        *mcp_servers = json!({});
    }

    mcp_servers.as_object_mut().unwrap().insert(
        "navifs".to_string(),
        json!({
            "command": exe_path,
            "args": ["mcp"]
        }),
    );

    let formatted = serde_json::to_string_pretty(&root)
        .context("Failed to serialize updated JSON config")?;

    fs::write(path, formatted)
        .with_context(|| format!("Failed to write updated config to {:?}", path))?;

    Ok(created)
}
