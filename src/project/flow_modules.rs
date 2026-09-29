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
    prefix: String,
    target: Option<PathBuf>,
    ambiguous: bool,
}

pub(super) struct Modules {
    root: PathBuf,
    pub parsed: BTreeMap<PathBuf, Parsed>,
    pub bindings: BTreeMap<PathBuf, BTreeMap<String, Binding>>,
    definitions: BTreeMap<PathBuf, BTreeMap<String, usize>>,
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

fn special_attribute(value: &str) -> bool {
    value.starts_with("__") && value.ends_with("__")
}

fn module_name(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() <= MODULE_LIMIT && parts.iter().all(|part| identifier(part))
}

fn imports(node: Node<'_>, source: &str, file: &Path) -> Option<Vec<(String, Binding)>> {
    let from = if node.kind() == "import_from_statement" {
        let module = node.child_by_field_name("module_name")?;
        let text: String = source[module.byte_range()]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let levels = text.chars().take_while(|c| *c == '.').count();
        let tail = &text[levels..];
        if !module_name(tail) && !(levels > 0 && tail.is_empty()) {
            return None;
        }
        if levels == 0 {
            Some(text)
        } else {
            let mut parent: Vec<_> = file
                .parent()?
                .iter()
                .map(|p| p.to_str())
                .collect::<Option<_>>()?;
            if levels > parent.len() {
                return None;
            }
            parent.truncate(parent.len() + 1 - levels);
            if !tail.is_empty() {
                parent.push(tail);
            }
            Some(parent.join("."))
        }
    } else {
        None
    };
    if from.as_ref().is_some_and(|name| !module_name(name)) {
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
        let name: String = source[name.byte_range()]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let explicit_alias = child.kind() == "aliased_import";
        let alias = if explicit_alias {
            &source[alias.byte_range()]
        } else {
            name.split('.').next()?
        };
        if !module_name(&name) || from.is_some() && !identifier(&name) || !identifier(alias) {
            return None;
        }
        result.push((
            alias.into(),
            Binding {
                module: from.clone().unwrap_or_else(|| name.clone()),
                member: from.as_ref().map(|_| name.clone()),
                prefix: if explicit_alias || from.is_some() {
                    alias.into()
                } else {
                    name
                },
                target: None,
                ambiguous: false,
            },
        ));
    }
    (!result.is_empty()).then_some(result)
}

fn candidate(project: &Project<'_>, path: &Path) -> Result<Value> {
    let absolute = project.root.join(path);
    let mut ancestor = project.root.clone();
    for part in path.parent().into_iter().flat_map(Path::iter) {
        ancestor.push(part);
        match std::fs::symlink_metadata(&ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Ok(json!({"status":"symlink"}))
            }
            Ok(metadata) if !metadata.is_dir() => return Ok(json!({"status":"outside-snapshot"})),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
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

#[derive(Serialize)]
struct Resolution {
    module: String,
    candidates: BTreeMap<PathBuf, Value>,
    packages: Vec<PathBuf>,
    target: Option<PathBuf>,
    admitted: bool,
}

fn resolve_module(project: &Project<'_>, name: &str) -> Result<Resolution> {
    ensure!(
        module_name(name),
        "import module name exceeds the admitted component budget."
    );
    let parts: Vec<_> = name.split('.').collect();
    let mut result = Resolution {
        module: name.into(),
        candidates: BTreeMap::new(),
        packages: Vec::new(),
        target: None,
        admitted: true,
    };
    for count in 1..=parts.len() {
        let stem = parts[..count].join("/");
        let module = PathBuf::from(format!("{stem}.py"));
        let package = PathBuf::from(format!("{stem}/__init__.py"));
        let stub = PathBuf::from(format!("{stem}.pyi"));
        let package_stub = PathBuf::from(format!("{stem}/__init__.pyi"));
        for path in [&module, &package, &stub, &package_stub] {
            result
                .candidates
                .insert(path.clone(), candidate(project, path)?);
        }
        let status = |path: &Path, expected| result.candidates[path]["status"] == expected;
        let stubs_missing = status(&stub, "missing") && status(&package_stub, "missing");
        let is_package = local_module_admitted(
            status(&package, "source"),
            status(&module, "missing"),
            stubs_missing,
        );
        let is_module = count == parts.len()
            && local_module_admitted(
                status(&module, "source"),
                status(&package, "missing"),
                stubs_missing,
            );
        result.admitted &= is_package || is_module;
        if count < parts.len() {
            result.packages.push(package.clone());
        } else if is_package {
            result.target = Some(package);
        } else if is_module {
            result.target = Some(module);
        }
    }
    if !result.admitted {
        result.target = None;
    }
    Ok(result)
}

fn entry_name(path: &Path) -> Option<String> {
    let mut parts: Vec<_> = path
        .with_extension("")
        .iter()
        .map(|p| p.to_str().map(str::to_owned))
        .collect::<Option<_>>()?;
    if parts.len() > 1 && parts.last().is_some_and(|p| p == "__init__") {
        parts.pop();
    }
    let name = parts.join(".");
    module_name(&name).then_some(name)
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

fn declares_member(
    project: &Project<'_>,
    path: &Path,
    member: &str,
    skip: Option<usize>,
) -> Result<bool> {
    let source = &project.sources[&project.root.join(path)];
    if source.len() > BYTE_LIMIT {
        return Ok(true);
    }
    let parsed = Parsers::new().parse(Language::Python, source)?;
    for node in parsed.root().named_children(&mut parsed.root().walk()) {
        if skip == Some(node.start_byte()) {
            continue;
        }
        if node.kind() == "function_definition"
            && node
                .child_by_field_name("name")
                .is_some_and(|name| &source[name.byte_range()] == member)
        {
            return Ok(true);
        }
        if matches!(node.kind(), "import_statement" | "import_from_statement")
            && imports(node, source, path)
                .is_some_and(|items| items.iter().any(|(alias, _)| alias == member))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

type MemberValue = (&'static str, Vec<(PathBuf, String)>, Option<PathBuf>);

impl Modules {
    fn member_chain(&self, file: &Path, name: &str) -> MemberValue {
        let mut file = file.to_owned();
        let mut name = name.to_owned();
        let mut chain = Vec::new();
        for _ in 0..MODULE_LIMIT {
            let item = (
                file.strip_prefix(&self.root).unwrap().to_owned(),
                name.clone(),
            );
            if chain.contains(&item) {
                return ("cyclic", chain, None);
            }
            chain.push(item);
            if !self.parsed.contains_key(&file) {
                return ("unavailable", chain, None);
            }
            let count = self
                .definitions
                .get(&file)
                .and_then(|items| items.get(&name))
                .copied()
                .unwrap_or(0);
            let binding = self.bindings.get(&file).and_then(|items| items.get(&name));
            if count > 1
                || count == 1 && binding.is_some()
                || binding.is_some_and(|binding| binding.ambiguous)
            {
                return ("ambiguous", chain, None);
            }
            if count == 1 {
                return ("function", chain, None);
            }
            let Some(binding) = binding else {
                return ("missing", chain, None);
            };
            let Some(target) = &binding.target else {
                return ("unavailable", chain, None);
            };
            if let Some(member) = &binding.member {
                file = self.root.join(target);
                name = member.clone();
            } else if binding.prefix == name && self.parsed.contains_key(&self.root.join(target)) {
                return ("module", chain, Some(target.clone()));
            } else {
                return ("unavailable", chain, None);
            }
        }
        ("budget", chain, None)
    }

    fn package_binding_conflicts(&self) -> bool {
        self.parsed.keys().any(|file| {
            let module = if file.file_name().is_some_and(|name| name == "__init__.py") {
                file.parent().unwrap()
            } else {
                file.as_path()
            };
            let Some(parent) = module.parent() else {
                return false;
            };
            let package = parent.join("__init__.py");
            let name = if file == module {
                module.file_stem()
            } else {
                module.file_name()
            };
            let Some(name) = name.and_then(|name| name.to_str()) else {
                return false;
            };
            self.definitions
                .get(&package)
                .is_some_and(|items| items.contains_key(name))
                || self.bindings.get(&package).is_some_and(|items| {
                    items.contains_key(name)
                        && self.member_chain(&package, name).2.as_deref()
                            != file.strip_prefix(&self.root).ok()
                })
        })
    }

    pub fn load(project: &Project<'_>, entry: &Path) -> Result<Self> {
        let name = entry_name(entry.strip_prefix(&project.root)?)
            .context("imported flow requires a root-local module or regular package entry.")?;
        let entry_resolution = resolve_module(project, &name)?;
        ensure!(
            entry_resolution.admitted
                && entry_resolution
                    .target
                    .as_ref()
                    .is_some_and(|path| project.root.join(path) == entry),
            "imported flow requires a root-local module or unambiguous regular package entry."
        );
        let mut result = Self {
            root: project.root.clone(),
            parsed: BTreeMap::new(),
            bindings: BTreeMap::new(),
            definitions: BTreeMap::new(),
            cutoffs: BTreeSet::new(),
            inputs: Value::Null,
        };
        if name.split('.').skip(1).any(special_attribute) {
            result.cutoffs.insert("special-module-attributes".into());
        }
        let mut pending = VecDeque::from([entry.to_owned()]);
        let mut queued = BTreeSet::from([entry.to_owned()]);
        let mut files = BTreeMap::new();
        let mut lookups = Vec::new();
        let mut edges: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
        for path in &entry_resolution.packages {
            let path = project.root.join(path);
            if queued.insert(path.clone()) {
                pending.push_back(path.clone());
            }
        }
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
            if parsed.has_errors() {
                result.cutoffs.insert("import-syntax-errors".into());
            }
            let mut bindings = BTreeMap::new();
            let mut names = BTreeSet::new();
            let mut definitions = BTreeMap::new();
            for node in parsed.root().named_children(&mut parsed.root().walk()) {
                if node.kind() == "function_definition" {
                    if let Some(name) = node.child_by_field_name("name") {
                        *definitions
                            .entry(source[name.byte_range()].to_owned())
                            .or_insert(0) += 1;
                        if !names.insert(source[name.byte_range()].to_owned()) {
                            result.cutoffs.insert("ambiguous-module-binding".into());
                        }
                    }
                    continue;
                }
                if !matches!(node.kind(), "import_statement" | "import_from_statement") {
                    continue;
                }
                let Some(imports) = imports(node, source, file.strip_prefix(&project.root)?) else {
                    result.cutoffs.insert("unsupported-import-form".into());
                    continue;
                };
                for (alias, mut binding) in imports {
                    import_count += 1;
                    if import_count > IMPORT_LIMIT {
                        result.cutoffs.insert("import-lookup-budget".into());
                        break;
                    }
                    if !names.insert(alias.clone()) {
                        binding.ambiguous = true;
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    }
                    if binding.module.split('.').skip(1).any(special_attribute)
                        || binding.member.as_deref().is_some_and(special_attribute)
                    {
                        result.cutoffs.insert("special-module-attributes".into());
                    }
                    let resolution = resolve_module(project, &binding.module)?;
                    let mut lookup = serde_json::to_value(&resolution)?;
                    lookup["importer"] = json!(file.strip_prefix(&project.root)?);
                    lookup["alias"] = json!(alias);
                    lookup["member"] = json!(binding.member);
                    lookup["prefix"] = json!(binding.prefix);
                    let submodule = if let (Some(target), Some(member)) =
                        (&resolution.target, &binding.member)
                    {
                        let skip = (project.root.join(target) == file).then_some(node.start_byte());
                        if target.as_path()
                            == Path::new(&format!(
                                "{}/__init__.py",
                                binding.module.replace('.', "/")
                            ))
                            && !special_attribute(member)
                            && !declares_member(project, target, member, skip)?
                        {
                            let child = format!("{}.{}", binding.module, member);
                            if module_name(&child) {
                                Some(resolve_module(project, &child)?)
                            } else {
                                result.cutoffs.insert("import-component-budget".into());
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    lookup["submodule"] = serde_json::to_value(&submodule)?;
                    let selected = submodule.as_ref().unwrap_or(&resolution);
                    for dependency in std::iter::once(&resolution).chain(submodule.iter()) {
                        if dependency.admitted {
                            for path in dependency.packages.iter().chain(&dependency.target) {
                                let absolute = project.root.join(path);
                                if dependency.target.as_ref() == Some(path)
                                    && !(submodule.is_some() && absolute == file)
                                {
                                    edges
                                        .entry(file.clone())
                                        .or_default()
                                        .insert(absolute.clone());
                                }
                                if queued.insert(absolute.clone()) {
                                    pending.push_back(absolute);
                                }
                            }
                        }
                    }
                    if selected.admitted {
                        binding.target = selected.target.clone();
                        if submodule.is_some() {
                            binding.member = None;
                        }
                    } else {
                        result
                            .cutoffs
                            .insert(format!("unresolved-local-import:{}", selected.module));
                    }
                    lookups.push(lookup);
                    bindings.insert(alias, binding);
                }
            }
            if names.iter().map(String::as_str).any(special_attribute) {
                result.cutoffs.insert("special-module-attributes".into());
            }
            files.insert(file.strip_prefix(&project.root)?.to_owned(), hash(source)?);
            result.bindings.insert(file.clone(), bindings);
            result.definitions.insert(file.clone(), definitions);
            result.parsed.insert(file, parsed);
        }
        let mut visited = BTreeSet::new();
        if result
            .parsed
            .keys()
            .any(|file| cyclic(file, &edges, &mut BTreeSet::new(), &mut visited))
        {
            result.cutoffs.insert("cyclic-module-initialization".into());
        }
        if result.package_binding_conflicts() {
            result
                .cutoffs
                .insert("package-child-binding-conflict".into());
        }
        for lookup in &mut lookups {
            let target = lookup["target"]
                .as_str()
                .map(|path| project.root.join(path));
            let fallback = lookup["submodule"]["target"].as_str().map(PathBuf::from);
            let (status, chain, module) = if !lookup["submodule"].is_null() {
                if let Some(path) =
                    fallback.filter(|path| result.parsed.contains_key(&project.root.join(path)))
                {
                    ("module", Vec::new(), Some(path))
                } else {
                    ("unavailable", Vec::new(), None)
                }
            } else {
                match (target, lookup["member"].as_str()) {
                    (Some(file), Some(member)) => result.member_chain(&file, member),
                    (Some(file), None) if result.parsed.contains_key(&file) => (
                        "module",
                        Vec::new(),
                        Some(file.strip_prefix(&project.root)?.to_owned()),
                    ),
                    _ => ("unavailable", Vec::new(), None),
                }
            };
            lookup["terminal_module"] = json!(module);
            lookup["resolution"] = json!(status);
            lookup["binding_chain"] = json!(chain
                .iter()
                .map(|(path, name)| { json!({"path":path,"name":name}) })
                .collect::<Vec<_>>());
            if !matches!(status, "function" | "module") {
                result.cutoffs.insert(format!(
                    "missing-or-ambiguous-import-member:{}",
                    lookup["alias"].as_str().unwrap()
                ));
            }
        }
        result.inputs = json!({"schema":"fr-flow-modules-4", "entry":entry_resolution,"files":files,"lookups":lookups,
            "complete":result.cutoffs.is_empty(),"cutoffs":result.cutoffs,
            "policy":"static workspace-local regular packages and .py modules; explicit acyclic function and module re-exports with package child fallback. No initialization effects, namespace packages, native modules, search paths or import hooks",
            "limits":{"modules":MODULE_LIMIT,"source_bytes":BYTE_LIMIT,"imports":IMPORT_LIMIT,"module_components":MODULE_LIMIT,"binding_chain":MODULE_LIMIT}});
        Ok(result)
    }

    pub fn resolve(&self, file: &Path, name: &str) -> String {
        let parts: Vec<_> = name.split('.').collect();
        if parts.is_empty()
            || parts.len() > MODULE_LIMIT
            || !parts.iter().all(|part| identifier(part))
        {
            return String::new();
        }
        let base = parts[0];
        let (mut target, mut members) =
            if let Some(binding) = self.bindings.get(file).and_then(|items| items.get(base)) {
                if binding.member.is_none() {
                    if binding.ambiguous {
                        return String::new();
                    }
                    let Some(target) = &binding.target else {
                        return String::new();
                    };
                    let Some(tail) = name.strip_prefix(&format!("{}.", binding.prefix)) else {
                        return String::new();
                    };
                    (self.root.join(target), tail.split('.').collect::<Vec<_>>())
                } else {
                    (file.to_owned(), parts)
                }
            } else {
                (file.to_owned(), parts)
            };
        while !members.is_empty() {
            let (status, chain, module) = self.member_chain(&target, members.remove(0));
            if status == "function" && members.is_empty() {
                let (path, name) = chain.last().unwrap();
                return format!("{}::{name}", path.display());
            }
            if let Some(module) = module {
                target = self.root.join(module);
            } else {
                break;
            }
        }
        String::new()
    }

    pub fn is_module(&self, file: &Path, name: &str) -> bool {
        self.member_chain(file, name).0 == "module"
    }

    pub fn function_name(&self, file: &Path, name: &str) -> String {
        format!(
            "{}::{name}",
            file.strip_prefix(&self.root)
                .expect("module belongs to workspace")
                .display()
        )
    }
}
