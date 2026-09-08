use super::{pattern, workspace_pattern_matches};
use std::path::{Component, Path, PathBuf};

pub(super) struct MemberPattern {
    base: PathBuf,
    parts: Vec<String>,
    literal_path: Option<PathBuf>,
}

impl MemberPattern {
    pub fn new(base: &Path, raw: &str, cargo: bool) -> Result<Self, &'static str> {
        if raw.is_empty() || raw.starts_with('/') {
            return Err("pattern-syntax-unsupported");
        }
        let declared: PathBuf = base
            .join(raw)
            .components()
            .filter(|part| *part != Component::CurDir)
            .collect();
        let mut base = base.to_path_buf();
        let mut components = raw.split('/').peekable();
        while let Some(&part) = components.peek() {
            match part {
                "" | "." => {}
                ".." if cargo => {
                    if !base.pop() {
                        return Err("pattern-outside-selected-root");
                    }
                }
                _ => break,
            }
            components.next();
        }
        let suffix = components.collect::<Vec<_>>().join("/");
        let parts = if suffix.is_empty() {
            Vec::new()
        } else {
            pattern(&suffix)?
        };
        let literal_path = (!parts.iter().any(|part| part == "*")).then_some(declared);
        Ok(Self {
            base,
            parts,
            literal_path,
        })
    }

    fn relative(&self, path: &Path) -> Option<Vec<String>> {
        Some(
            path.strip_prefix(&self.base)
                .ok()?
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect(),
        )
    }

    pub fn matches(&self, directory: &Path) -> bool {
        self.relative(directory)
            .is_some_and(|parts| workspace_pattern_matches(&self.parts, &parts))
    }

    pub fn literal_prefix(&self, manifest: &Path) -> bool {
        self.literal_path
            .as_ref()
            .is_some_and(|path| manifest.starts_with(path))
    }
}
