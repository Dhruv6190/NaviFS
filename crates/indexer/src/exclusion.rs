//! Path exclusion and ignore pattern evaluation for scanner and live watcher

use std::path::{Component, Path};
use tracing::trace;

/// Engine for determining whether a given filesystem path should be excluded from scanning & indexing
#[derive(Debug, Clone)]
pub struct PathExclusionFilter {
    ignore_patterns: Vec<String>,
    ignore_hidden: bool,
}

impl Default for PathExclusionFilter {
    fn default() -> Self {
        Self::new(Vec::new(), true)
    }
}

impl PathExclusionFilter {
    pub fn new(custom_patterns: Vec<String>, ignore_hidden: bool) -> Self {
        let mut patterns = vec![
            "node_modules".to_string(),
            ".git".to_string(),
            ".svn".to_string(),
            ".hg".to_string(),
            "target".to_string(),
            ".venv".to_string(),
            "venv".to_string(),
            ".navifs".to_string(),
            "dist".to_string(),
            "build".to_string(),
            "__pycache__".to_string(),
            ".DS_Store".to_string(),
            "Thumbs.db".to_string(),
            "desktop.ini".to_string(),
            ".idea".to_string(),
            ".vscode".to_string(),
            ".vcf".to_string(),
            ".vfc".to_string(),
            ".ics".to_string(),
        ];

        for custom in custom_patterns {
            let trimmed = custom.trim().to_string();
            if !trimmed.is_empty() && !patterns.contains(&trimmed) {
                patterns.push(trimmed);
            }
        }

        Self {
            ignore_patterns: patterns,
            ignore_hidden,
        }
    }

    /// Evaluates if a given path should be skipped
    pub fn should_exclude(&self, path: &Path) -> bool {
        // 1. Check for hidden components (starting with .)
        if self.ignore_hidden {
            for comp in path.components() {
                if let Component::Normal(os_str) = comp {
                    let s = os_str.to_string_lossy();
                    if s.starts_with('.') && s != "." && s != ".." {
                        trace!("Excluding hidden component '{}' in {:?}", s, path);
                        return true;
                    }
                }
            }
        }

        // 2. Check path against normalized pattern list
        let path_str = path.to_string_lossy().replace('\\', "/");
        let path_segments: Vec<&str> = path_str.split('/').filter(|s| !s.is_empty()).collect();

        for pattern in &self.ignore_patterns {
            let clean_pat = pattern
                .trim_matches('*')
                .trim_matches('/')
                .replace('\\', "/");

            // Segment-exact match (e.g., "node_modules", "target")
            if path_segments
                .iter()
                .any(|seg| seg.eq_ignore_ascii_case(&clean_pat))
            {
                trace!("Excluding path {:?} matching segment '{}'", path, pattern);
                return true;
            }

            // Substring or suffix match (e.g., ".tmp", ".DS_Store", ".vcf")
            let path_str_lower = path_str.to_ascii_lowercase();
            let clean_pat_lower = clean_pat.to_ascii_lowercase();
            if path_str_lower.ends_with(&clean_pat_lower) || path_str_lower.contains(&format!("/{}/", clean_pat_lower)) {
                trace!("Excluding path {:?} matching pattern '{}'", path, pattern);
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_exclusions() {
        let filter = PathExclusionFilter::default();

        assert!(filter.should_exclude(Path::new("C:/project/node_modules/package/index.js")));
        assert!(filter.should_exclude(Path::new("C:/project/.git/objects/12345")));
        assert!(filter.should_exclude(Path::new("C:/project/target/debug/build.exe")));
        assert!(filter.should_exclude(Path::new("C:/project/.navifs/navifs.db")));
        assert!(filter.should_exclude(Path::new("C:/project/sub/.hidden_dir/file.txt")));
        assert!(filter.should_exclude(Path::new("C:/project/Thumbs.db")));

        // Valid files should not be excluded
        assert!(!filter.should_exclude(Path::new("C:/project/src/main.rs")));
        assert!(!filter.should_exclude(Path::new("C:/project/docs/README.md")));
        assert!(!filter.should_exclude(Path::new("C:/project/data/report.pdf")));
    }

    #[test]
    fn test_custom_exclusions() {
        let filter =
            PathExclusionFilter::new(vec!["custom_logs".to_string(), "*.cache".to_string()], true);

        assert!(filter.should_exclude(Path::new("C:/project/custom_logs/app.log")));
        assert!(filter.should_exclude(Path::new("C:/project/file.cache")));
        assert!(!filter.should_exclude(Path::new("C:/project/src/service.rs")));
    }
}
