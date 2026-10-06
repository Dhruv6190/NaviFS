//! `navifs setup`: registers NaviFS as an MCP server in the AI clients installed on this machine.
//!
//! Safety rules: only detected clients are touched, existing config files are never
//! overwritten when they cannot be parsed, every modified file is backed up first, and writes
//! are atomic (temp file + rename).

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use directories::BaseDirs;
use serde_json::{json, Map, Value};

const SERVER_KEY: &str = "navifs";

/// An AI client that can load MCP servers from a JSON config file
#[derive(Debug, Clone)]
pub struct ClientTarget {
    pub id: &'static str,
    pub name: &'static str,
    pub config_path: PathBuf,
}

impl ClientTarget {
    /// A client counts as installed when the directory that holds its config exists
    pub fn is_detected(&self) -> bool {
        self.config_path.parent().is_some_and(Path::exists)
    }
}

/// What `navifs setup` should do
#[derive(Debug, Clone, Default)]
pub struct SetupOptions {
    /// Configure every detected client without prompting
    pub all: bool,
    /// Configure only this client (id or name fragment)
    pub client: Option<String>,
    /// Print what would change without touching any file
    pub dry_run: bool,
    /// Remove the NaviFS entry instead of adding it
    pub uninstall: bool,
}

/// Supported clients, with config locations derived from OS conventions
pub fn supported_clients() -> Result<Vec<ClientTarget>> {
    let dirs = BaseDirs::new().context("could not determine the user home directory")?;
    let home = dirs.home_dir();
    // %APPDATA% on Windows, ~/Library/Application Support on macOS, ~/.config on Linux
    let config = dirs.config_dir();

    Ok(vec![
        ClientTarget {
            id: "claude",
            name: "Claude Desktop",
            config_path: config.join("Claude").join("claude_desktop_config.json"),
        },
        ClientTarget {
            id: "cursor",
            name: "Cursor",
            config_path: home.join(".cursor").join("mcp.json"),
        },
        ClientTarget {
            id: "antigravity",
            name: "Antigravity",
            config_path: home.join(".gemini").join("config").join("mcp_config.json"),
        },
        ClientTarget {
            id: "windsurf",
            name: "Windsurf",
            config_path: home
                .join(".codeium")
                .join("windsurf")
                .join("mcp_config.json"),
        },
        ClientTarget {
            id: "vscode",
            name: "VS Code (Cline / Roo Code)",
            config_path: config
                .join("Code")
                .join("User")
                .join("globalStorage")
                .join("saoudrizwan.claude-dev")
                .join("settings")
                .join("cline_mcp_settings.json"),
        },
    ])
}

pub fn run_setup(options: SetupOptions) -> Result<()> {
    let exe = current_exe_string()?;
    let clients = supported_clients()?;

    let selected = select_clients(&clients, &options)?;
    if selected.is_empty() {
        bail!(
            "no supported AI client was detected. Supported: {}. \
             Use --client <name> to configure one explicitly.",
            clients.iter().map(|c| c.id).collect::<Vec<_>>().join(", ")
        );
    }

    let verb = if options.uninstall {
        "Removing from"
    } else {
        "Configuring"
    };
    println!(
        "NaviFS: {verb} {} client(s){}",
        selected.len(),
        if options.dry_run { " (dry run)" } else { "" }
    );

    let mut failures = 0;
    for client in selected {
        let outcome = if options.uninstall {
            remove_entry(&client.config_path, options.dry_run)
        } else {
            add_entry(&client.config_path, &exe, options.dry_run)
        };
        match outcome {
            Ok(change) => println!(
                "  [ok] {:<28} {} ({})",
                client.name,
                client.config_path.display(),
                change.describe()
            ),
            Err(e) => {
                failures += 1;
                eprintln!(
                    "  [!!] {:<28} {}: {e:#}",
                    client.name,
                    client.config_path.display()
                );
            }
        }
    }

    if failures > 0 {
        bail!("{failures} client(s) could not be configured; their files were left untouched");
    }
    if !options.dry_run && !options.uninstall {
        println!("\nDone. Restart your AI client to load the NaviFS MCP server.");
    }
    Ok(())
}

fn current_exe_string() -> Result<String> {
    let exe = std::env::current_exe().context("could not determine the NaviFS executable path")?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let s = exe.to_string_lossy().to_string();
    // Strip the Windows verbatim prefix; many MCP clients cannot launch `\\?\C:\...` paths
    Ok(s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s))
}

fn select_clients<'a>(
    clients: &'a [ClientTarget],
    options: &SetupOptions,
) -> Result<Vec<&'a ClientTarget>> {
    if let Some(wanted) = &options.client {
        let wanted = wanted.to_lowercase();
        let client = clients
            .iter()
            .find(|c| c.id == wanted || c.name.to_lowercase().contains(&wanted))
            .with_context(|| {
                format!(
                    "unknown client '{wanted}'. Supported: {}",
                    clients.iter().map(|c| c.id).collect::<Vec<_>>().join(", ")
                )
            })?;
        return Ok(vec![client]);
    }

    let detected: Vec<&ClientTarget> = clients.iter().filter(|c| c.is_detected()).collect();
    if options.all || detected.is_empty() {
        return Ok(detected);
    }
    if !io::stdin().is_terminal() {
        bail!("not running in an interactive terminal; pass --all or --client <name>");
    }

    println!("Detected AI clients:\n");
    for (i, client) in detected.iter().enumerate() {
        println!("  [{}] {}", i + 1, client.name);
    }
    print!("\nSelect clients (e.g. '1,3'), or press Enter for all: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if input.is_empty() || input.eq_ignore_ascii_case("a") {
        return Ok(detected);
    }

    let mut chosen = Vec::new();
    for part in input.split([',', ' ']).filter(|p| !p.is_empty()) {
        let n: usize = part
            .parse()
            .with_context(|| format!("invalid selection '{part}'"))?;
        let client = detected
            .get(n.wrapping_sub(1))
            .with_context(|| format!("selection {n} is out of range"))?;
        chosen.push(*client);
    }
    Ok(chosen)
}

/// Result of applying a change to one config file
#[derive(Debug, PartialEq, Eq)]
enum Change {
    Created,
    Updated,
    Unchanged,
    Removed,
    NotPresent,
}

impl Change {
    fn describe(&self) -> &'static str {
        match self {
            Change::Created => "created",
            Change::Updated => "updated",
            Change::Unchanged => "already up to date",
            Change::Removed => "removed",
            Change::NotPresent => "nothing to remove",
        }
    }
}

fn entry_for(exe: &str) -> Value {
    json!({ "command": exe, "args": ["mcp"] })
}

/// Reads a JSON object config. A missing file yields `None`; an unreadable or non-object file
/// is an error so user settings are never silently discarded.
fn read_config(path: &Path) -> Result<Option<Map<String, Value>>> {
    if !path.exists() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(Some(Map::new()));
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(Some(map)),
        Ok(_) => bail!("top level of the file is not a JSON object; fix it manually and re-run"),
        Err(e) => bail!("existing file is not valid JSON ({e}); fix it manually and re-run"),
    }
}

fn add_entry(path: &Path, exe: &str, dry_run: bool) -> Result<Change> {
    let existing = read_config(path)?;
    let created = existing.is_none();
    let mut root = existing.unwrap_or_default();

    let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
    let Value::Object(servers) = servers else {
        bail!("\"mcpServers\" exists but is not an object; fix it manually and re-run");
    };

    let desired = entry_for(exe);
    if servers.get(SERVER_KEY) == Some(&desired) {
        return Ok(Change::Unchanged);
    }
    servers.insert(SERVER_KEY.to_string(), desired);

    if !dry_run {
        write_atomic(path, &Value::Object(root), !created)?;
    }
    Ok(if created {
        Change::Created
    } else {
        Change::Updated
    })
}

fn remove_entry(path: &Path, dry_run: bool) -> Result<Change> {
    let Some(mut root) = read_config(path)? else {
        return Ok(Change::NotPresent);
    };
    let removed = root
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .and_then(|servers| servers.remove(SERVER_KEY))
        .is_some();
    if !removed {
        return Ok(Change::NotPresent);
    }
    if !dry_run {
        write_atomic(path, &Value::Object(root), true)?;
    }
    Ok(Change::Removed)
}

/// Writes `value` to `path` via a temp file in the same directory, keeping a `.navifs.bak`
/// copy of any previous content.
fn write_atomic(path: &Path, value: &Value, backup: bool) -> Result<()> {
    let dir = path
        .parent()
        .context("config path has no parent directory")?;
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;

    if backup && path.exists() {
        let mut backup_path = path.as_os_str().to_owned();
        backup_path.push(".navifs.bak");
        fs::copy(path, &backup_path)
            .with_context(|| format!("failed to back up {}", path.display()))?;
    }

    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".navifs.tmp");
    let tmp = PathBuf::from(tmp);
    let text = serde_json::to_string_pretty(value).context("failed to serialise config")?;
    fs::write(&tmp, text + "\n").with_context(|| format!("failed to write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn creates_missing_config_with_navifs_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("mcp.json");

        assert_eq!(
            add_entry(&path, "/bin/navifs", false).unwrap(),
            Change::Created
        );
        let json = read(&path);
        assert_eq!(json["mcpServers"]["navifs"]["command"], "/bin/navifs");
        assert_eq!(json["mcpServers"]["navifs"]["args"], json!(["mcp"]));
    }

    #[test]
    fn preserves_existing_servers_and_settings_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(
            &path,
            r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#,
        )
        .unwrap();

        assert_eq!(
            add_entry(&path, "/bin/navifs", false).unwrap(),
            Change::Updated
        );
        let json = read(&path);
        assert_eq!(json["theme"], "dark");
        assert_eq!(json["mcpServers"]["other"]["command"], "x");
        assert_eq!(json["mcpServers"]["navifs"]["command"], "/bin/navifs");
        assert!(dir.path().join("mcp.json.navifs.bak").exists());
    }

    #[test]
    fn invalid_json_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        let original = "{ // user comment\n \"mcpServers\": {} }";
        fs::write(&path, original).unwrap();

        assert!(add_entry(&path, "/bin/navifs", false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn rerun_with_same_binary_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        add_entry(&path, "/bin/navifs", false).unwrap();
        assert_eq!(
            add_entry(&path, "/bin/navifs", false).unwrap(),
            Change::Unchanged
        );
    }

    #[test]
    fn dry_run_does_not_touch_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        assert_eq!(
            add_entry(&path, "/bin/navifs", true).unwrap(),
            Change::Created
        );
        assert!(!path.exists());
    }

    #[test]
    fn uninstall_removes_only_the_navifs_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(
            &path,
            r#"{"mcpServers":{"other":{"command":"x"},"navifs":{"command":"y"}}}"#,
        )
        .unwrap();

        assert_eq!(remove_entry(&path, false).unwrap(), Change::Removed);
        let json = read(&path);
        assert!(json["mcpServers"].get("navifs").is_none());
        assert_eq!(json["mcpServers"]["other"]["command"], "x");
        assert_eq!(remove_entry(&path, false).unwrap(), Change::NotPresent);
    }
}
