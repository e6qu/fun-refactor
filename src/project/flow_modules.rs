use super::*;
use crate::parse::Parsed;
use std::path::{Path, PathBuf};

const MODULE_LIMIT: usize = 16;
const BYTE_LIMIT: usize = 262_144;
const IMPORT_LIMIT: usize = 128;

#[derive(Clone, Serialize)]
pub(super) struct Binding {
    module: String,
    pub member: Option<String>,
}

pub(super) struct Modules {
    pub parsed: BTreeMap<PathBuf, Parsed>,
    pub bindings: BTreeMap<PathBuf, BTreeMap<String, Binding>>,
    pub cutoffs: BTreeSet<String>,
    pub inputs: Value,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || i > 0 && c.is_ascii_digit())
}

fn imports(node: Node<'_>, source: &str) -> Option<Vec<(String, Binding)>> {
    let from = if node.kind() == "import_from_statement" {
        let module = node.child_by_field_name("module_name")?;
        Some(source[module.byte_range()].to_owned())
    } else {
        None
    };
    if from.as_ref().is_some_and(|name| !identifier(name)) {
        return None;
    }
    let mut result = Vec::new();
    for child in node.children_by_field_name("name", &mut node.walk()) {
        let (name, alias) = if child.kind() == "aliased_import" {
            (
                child.child_by_field_name("name")?,
                child.child_by_field_name("alias")?,
            )
        } else {
            (child, child)
        };
        let name = &source[name.byte_range()];
        let alias = &source[alias.byte_range()];
        if !identifier(name) || !identifier(alias) {
            return None;
        }
        result.push((
            alias.into(),
            Binding {
                module: from.clone().unwrap_or_else(|| name.into()),
                member: from.as_ref().map(|_| name.into()),
            },
        ));
    }
    (!result.is_empty()).then_some(result)
}

fn candidate(project: &Project<'_>, path: &Path) -> Result<Value> {
    let absolute = project.root.join(path);
    let metadata = match std::fs::symlink_metadata(&absolute) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if metadata
        .as_ref()
        .is_some_and(|m| m.file_type().is_symlink())
    {
        return Ok(json!({"status":"symlink"}));
    }
    if let Some(source) = project.sources.get(&absolute) {
        return Ok(json!({"status":"source","digest":hash(source)?}));
    }
    Ok(json!({"status":if metadata.is_some() {"outside-snapshot"} else {"missing"}}))
}

fn cyclic(
    file: &Path,
    edges: &BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    active: &mut BTreeSet<PathBuf>,
    visited: &mut BTreeSet<PathBuf>,
) -> bool {
    if active.contains(file) {
        return true;
    }
    if !visited.insert(file.to_owned()) {
        return false;
    }
    active.insert(file.to_owned());
    let result = edges.get(file).is_some_and(|children| {
        children
            .iter()
            .any(|child| cyclic(child, edges, active, visited))
    });
    active.remove(file);
    result
}

impl Modules {
    pub fn load(project: &Project<'_>, entry: &Path) -> Result<Self> {
        ensure!(
            entry.parent() == Some(project.root.as_path()),
            "imported flow requires a root-local entry module"
        );
        let mut result = Self {
            parsed: BTreeMap::new(),
            bindings: BTreeMap::new(),
            cutoffs: BTreeSet::new(),
            inputs: Value::Null,
        };
        let mut pending = VecDeque::from([entry.to_owned()]);
        let mut queued = BTreeSet::from([entry.to_owned()]);
        let mut files = BTreeMap::new();
        let mut lookups = Vec::new();
        let mut edges: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
        let mut bytes = 0;
        let mut import_count = 0;
        while let Some(file) = pending.pop_front() {
            if result.parsed.len() == MODULE_LIMIT {
                result.cutoffs.insert("import-module-budget".into());
                break;
            }
            let source = &project.sources[&file];
            bytes += source.len();
            if bytes > BYTE_LIMIT {
                result.cutoffs.insert("import-source-byte-budget".into());
                break;
            }
            let parsed = Parsers::new().parse(Language::Python, source)?;
            if parsed.root().has_error() {
                result.cutoffs.insert("import-syntax-errors".into());
            }
            let mut bindings = BTreeMap::new();
            let mut names = BTreeSet::new();
            for node in parsed.root().named_children(&mut parsed.root().walk()) {
                if node.kind() == "function_definition" {
                    if let Some(name) = node.child_by_field_name("name") {
                        if !names.insert(source[name.byte_range()].to_owned()) {
                            result.cutoffs.insert("ambiguous-module-binding".into());
                        }
                    }
                    continue;
                }
                if !matches!(node.kind(), "import_statement" | "import_from_statement") {
                    continue;
                }
                let Some(imports) = imports(node, source) else {
                    result.cutoffs.insert("unsupported-import-form".into());
                    continue;
                };
                for (alias, binding) in imports {
                    import_count += 1;
                    if import_count > IMPORT_LIMIT {
                        result.cutoffs.insert("import-lookup-budget".into());
                        break;
                    }
                    if !names.insert(alias.clone()) {
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    }
                    let module = PathBuf::from(format!("{}.py", binding.module));
                    let package = PathBuf::from(format!("{}/__init__.py", binding.module));
                    let stub = PathBuf::from(format!("{}.pyi", binding.module));
                    let candidates = [module.clone(), package, stub]
                        .into_iter()
                        .map(|path| Ok((path.clone(), candidate(project, &path)?)))
                        .collect::<Result<BTreeMap<_, _>>>()?;
                    let admitted = local_module_admitted(
                        candidates[&module]["status"] == "source",
                        candidates[&PathBuf::from(format!("{}/__init__.py", binding.module))]
                            ["status"]
                            == "missing",
                        candidates[&PathBuf::from(format!("{}.pyi", binding.module))]["status"]
                            == "missing",
                    );
                    lookups.push(json!({"importer":file.strip_prefix(&project.root)?,
                        "alias":alias,"module":binding.module,"member":binding.member,
                        "candidates":candidates,"admitted":admitted}));
                    if admitted {
                        let absolute = project.root.join(&module);
                        edges
                            .entry(file.clone())
                            .or_default()
                            .insert(absolute.clone());
                        if queued.insert(absolute.clone()) {
                            pending.push_back(absolute);
                        }
                    } else {
                        result
                            .cutoffs
                            .insert(format!("unresolved-local-import:{}", binding.module));
                    }
                    bindings.insert(alias, binding);
                }
            }
            files.insert(file.strip_prefix(&project.root)?.to_owned(), hash(source)?);
            result.bindings.insert(file.clone(), bindings);
            result.parsed.insert(file, parsed);
        }
        if cyclic(entry, &edges, &mut BTreeSet::new(), &mut BTreeSet::new()) {
            result.cutoffs.insert("cyclic-module-initialization".into());
        }
        for lookup in &mut lookups {
            let file = project
                .root
                .join(format!("{}.py", lookup["module"].as_str().unwrap()));
            let status = if let Some(parsed) = result.parsed.get(&file) {
                if let Some(member) = lookup["member"].as_str() {
                    let source = &project.sources[&file];
                    let count = parsed
                        .root()
                        .named_children(&mut parsed.root().walk())
                        .filter(|node| {
                            node.kind() == "function_definition"
                                && node
                                    .child_by_field_name("name")
                                    .is_some_and(|name| &source[name.byte_range()] == member)
                        })
                        .count();
                    match count {
                        0 => "missing",
                        1 => "function",
                        _ => "ambiguous",
                    }
                } else {
                    "module"
                }
            } else {
                "unavailable"
            };
            lookup["resolution"] = json!(status);
            if matches!(status, "missing" | "ambiguous") {
                result.cutoffs.insert(format!(
                    "missing-or-ambiguous-import-member:{}",
                    lookup["alias"].as_str().unwrap()
                ));
            }
        }
        result.inputs = json!({"schema":"fr-flow-modules-1", "files":files,"lookups":lookups,
            "complete":result.cutoffs.is_empty(),"cutoffs":result.cutoffs,
            "policy":"static root-local .py modules; no packages, native modules, search paths or import hooks",
            "limits":{"modules":MODULE_LIMIT,"source_bytes":BYTE_LIMIT,"imports":IMPORT_LIMIT}});
        Ok(result)
    }

    pub fn resolve(&self, file: &Path, name: &str) -> String {
        let (base, member) = name
            .split_once('.')
            .map_or((name, None), |(a, b)| (a, Some(b)));
        if let Some(binding) = self.bindings.get(file).and_then(|items| items.get(base)) {
            let member = match (&binding.member, member) {
                (Some(member), None) => member.as_str(),
                (None, Some(member)) => member,
                _ => return String::new(),
            };
            format!("{}.py::{member}", binding.module)
        } else if member.is_none() {
            format!("{}::{name}", file.file_name().unwrap().to_string_lossy())
        } else {
            String::new()
        }
    }
}
