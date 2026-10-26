use fun_refactor::index::Index;
use fun_refactor::lang::Language;

pub fn ignored_rcases_wildcards(index: &Index, root: &std::path::Path) -> Vec<serde_json::Value> {
    let pattern = regex::Regex::new(r"(?m)\brcases\b[^\n]*?\bwith[ \t]+(_)[ \t]*\|").unwrap();
    let mut sites = Vec::new();
    for (path, file) in index
        .files()
        .filter(|(_, file)| file.language == Language::Lean)
    {
        let absolute = if path.is_absolute() {
            path.clone()
        } else {
            root.join(path)
        };
        let source = std::fs::read_to_string(&absolute).unwrap();
        for capture in pattern.captures_iter(&source) {
            let wildcard = capture.get(1).unwrap();
            assert!(file
                .references
                .iter()
                .all(|index_in_file| !index.references[*index_in_file]
                    .span
                    .contains_offset(wildcard.start())));
            sites.push(
                serde_json::json!({"file": absolute.strip_prefix(root).unwrap(),
                "span": {"start": wildcard.start(), "end": wildcard.end()}}),
            );
        }
    }
    sites
}
