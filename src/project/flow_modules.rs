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
    pub future: bool,
    prefix: String,
    target: Option<PathBuf>,
    ambiguous: bool,
}

pub(super) struct Modules {
    root: PathBuf,
    pub parsed: BTreeMap<PathBuf, Parsed>,
    namespaces: BTreeSet<PathBuf>,
    pub bindings: BTreeMap<PathBuf, BTreeMap<String, Binding>>,
    definitions: BTreeMap<PathBuf, BTreeMap<String, usize>>,
    literals: BTreeMap<PathBuf, BTreeSet<String>>,
    opaque_bindings: BTreeMap<PathBuf, BTreeSet<String>>,
    future_nodes: BTreeMap<PathBuf, BTreeSet<usize>>,
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
    let from = if node.kind() == "future_import_statement" {
        Some("__future__".to_owned())
    } else if node.kind() == "import_from_statement" {
        let module = node.child_by_field_name("module_name")?;
        let text: String = source[module.byte_range()]
            .replace("\\\r\n", "")
            .replace("\\\n", "")
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
    if node
        .named_children(&mut node.walk())
        .any(|child| child.kind() == "wildcard_import")
    {
        return Some(vec![(
            "*".into(),
            Binding {
                module: from?,
                member: None,
                prefix: "*".into(),
                target: None,
                ambiguous: false,
                future: false,
            },
        )]);
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
            .replace("\\\r\n", "")
            .replace("\\\n", "")
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
                future: false,
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
    Ok(
        json!({"status":if metadata.as_ref().is_some_and(|m| m.is_dir()) {"directory"}
        else if metadata.is_some() {"outside-snapshot"} else {"missing"}}),
    )
}

#[derive(Serialize)]
struct Resolution {
    module: String,
    candidates: BTreeMap<PathBuf, Value>,
    packages: Vec<PathBuf>,
    namespaces: Vec<PathBuf>,
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
        namespaces: Vec::new(),
        target: None,
        admitted: true,
    };
    for count in 1..=parts.len() {
        let stem = parts[..count].join("/");
        let module = PathBuf::from(format!("{stem}.py"));
        let package = PathBuf::from(format!("{stem}/__init__.py"));
        let stub = PathBuf::from(format!("{stem}.pyi"));
        let package_stub = PathBuf::from(format!("{stem}/__init__.pyi"));
        let directory = PathBuf::from(&stem);
        for path in [&module, &package, &stub, &package_stub, &directory] {
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
        let is_namespace = local_module_admitted(
            status(&directory, "directory"),
            status(&module, "missing"),
            status(&package, "missing") && stubs_missing,
        );
        result.admitted &= is_package || is_module || is_namespace;
        if is_namespace {
            result.namespaces.push(directory.clone());
        }
        if count < parts.len() && is_package {
            result.packages.push(package.clone());
        } else if count == parts.len() && is_package {
            result.target = Some(package);
        } else if is_module {
            result.target = Some(module);
        } else if count == parts.len() && is_namespace {
            result.target = Some(directory);
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

fn opaque_names<'s>(node: Node<'_>, source: &'s str) -> Vec<&'s str> {
    fn targets<'s>(node: Node<'_>, source: &'s str, names: &mut Vec<&'s str>) {
        match node.kind() {
            "identifier" => names.push(&source[node.byte_range()]),
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "list_splat_pattern"
            | "parenthesized_expression" => {
                for child in node.named_children(&mut node.walk()) {
                    targets(child, source, names);
                }
            }
            _ => (),
        }
    }
    let mut names = Vec::new();
    match node.kind() {
        "class_definition" => {
            if let Some(name) = node.child_by_field_name("name") {
                names.push(&source[name.byte_range()]);
            }
        }
        "decorated_definition" => {
            if let Some(definition) = node.child_by_field_name("definition") {
                if let Some(name) = definition.child_by_field_name("name") {
                    names.push(&source[name.byte_range()]);
                }
            }
        }
        "expression_statement" => {
            for child in node.named_children(&mut node.walk()) {
                names.extend(opaque_names(child, source));
            }
        }
        "assignment" | "augmented_assignment" => {
            if node.kind() == "assignment" && node.child_by_field_name("right").is_none() {
                return names;
            }
            if let Some(left) = node.child_by_field_name("left") {
                targets(left, source, &mut names);
            }
            if let Some(right) = node.child_by_field_name("right") {
                if right.kind() == "assignment" {
                    names.extend(opaque_names(right, source));
                }
            }
        }
        _ => (),
    }
    names
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
        if opaque_names(node, source).contains(&member) {
            return Ok(true);
        }
        if matches!(
            node.kind(),
            "import_statement" | "import_from_statement" | "future_import_statement"
        ) {
            if let Some(items) = imports(node, source, path) {
                for (alias, binding) in items {
                    if alias == member {
                        return Ok(true);
                    }
                    if alias == "*" {
                        let resolution = resolve_module(project, &binding.module)?;
                        let exports = resolution
                            .target
                            .as_ref()
                            .map(|target| export_names(project, target))
                            .transpose()?
                            .flatten();
                        if exports.is_none_or(|names| names.contains(member)) {
                            return Ok(true);
                        }
                    }
                }
            }
        }
    }
    Ok(false)
}

fn export_names(project: &Project<'_>, target: &Path) -> Result<Option<BTreeSet<String>>> {
    let Some(source) = project
        .sources
        .get(&project.root.join(target))
        .filter(|s| s.len() <= BYTE_LIMIT)
    else {
        return Ok(None);
    };
    let parsed = Parsers::new().parse(Language::Python, source)?;
    Ok((!parsed.has_errors())
        .then(|| calls::declared_exports(parsed.root(), source))
        .flatten())
}

type MemberValue = (&'static str, Vec<(PathBuf, String)>, Option<PathBuf>);

impl Modules {
    pub(super) fn annotations_postponed(&self, file: &Path) -> bool {
        self.future_nodes
            .get(file)
            .is_some_and(|nodes| !nodes.is_empty())
    }

    pub(super) fn future_statement_is_inert(&self, file: &Path, start: usize) -> bool {
        self.future_nodes
            .get(file)
            .is_some_and(|nodes| nodes.contains(&start))
    }

    fn known_module(&self, path: &Path) -> bool {
        self.parsed.contains_key(path) || self.namespaces.contains(path)
    }

    fn retain_namespaces(&mut self, root: &Path, paths: &[PathBuf]) {
        for path in paths {
            let absolute = root.join(path);
            if self.namespaces.contains(&absolute) {
                continue;
            }
            if self.parsed.len() + self.namespaces.len() + 1 >= MODULE_LIMIT {
                self.cutoffs.insert("import-module-budget".into());
                break;
            }
            self.namespaces.insert(absolute);
        }
    }
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
            if !self.known_module(&file) {
                return ("unavailable", chain, None);
            }
            let count = self
                .definitions
                .get(&file)
                .and_then(|items| items.get(&name))
                .copied()
                .unwrap_or(0);
            let binding = self.bindings.get(&file).and_then(|items| items.get(&name));
            if self
                .literals
                .get(&file)
                .is_some_and(|names| names.contains(&name))
                || self
                    .opaque_bindings
                    .get(&file)
                    .is_some_and(|names| names.contains(&name))
            {
                return ("unavailable", chain, None);
            }
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
            } else if binding.prefix == name && self.known_module(&self.root.join(target)) {
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
                || self
                    .literals
                    .get(&package)
                    .is_some_and(|items| items.contains(name))
                || self
                    .opaque_bindings
                    .get(&package)
                    .is_some_and(|items| items.contains(name))
                || self.bindings.get(&package).is_some_and(|items| {
                    items.contains_key(name)
                        && self.member_chain(&package, name).2.as_deref()
                            != file.strip_prefix(&self.root).ok()
                })
        })
    }

    pub fn load(project: &Project<'_>, entry: &Path) -> Result<Self> {
        let name = entry_name(entry.strip_prefix(&project.root)?)
            .context("imported flow requires a root-local Python source entry.")?;
        let entry_resolution = resolve_module(project, &name)?;
        ensure!(
            entry_resolution.admitted
                && entry_resolution
                    .target
                    .as_ref()
                    .is_some_and(|path| project.root.join(path) == entry),
            "imported flow requires an unambiguous root-local Python source entry."
        );
        let mut result = Self {
            root: project.root.clone(),
            parsed: BTreeMap::new(),
            namespaces: BTreeSet::new(),
            bindings: BTreeMap::new(),
            definitions: BTreeMap::new(),
            literals: BTreeMap::new(),
            opaque_bindings: BTreeMap::new(),
            future_nodes: BTreeMap::new(),
            cutoffs: BTreeSet::new(),
            inputs: Value::Null,
        };
        result.retain_namespaces(&project.root, &entry_resolution.namespaces);
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
            if result.parsed.len() + result.namespaces.len() >= MODULE_LIMIT {
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
            let mut literals = BTreeSet::new();
            let mut opaque_bindings = BTreeSet::new();
            let declared_exports = calls::declared_exports(parsed.root(), source);
            let future_nodes = calls::future_annotation_nodes(parsed.root(), source);
            for node in parsed.root().named_children(&mut parsed.root().walk()) {
                if calls::module_export_list(node, source).is_some() {
                    literals.insert("__all__".to_owned());
                    if !names.insert("__all__".to_owned()) {
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    }
                    continue;
                }
                if let Some(name) = calls::module_literal(node, source)
                    .or_else(|| calls::module_metadata(node, source))
                {
                    literals.insert(name.to_owned());
                    if !names.insert(name.to_owned()) {
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    }
                    continue;
                }
                if node.kind() == "function_definition" {
                    if let Some(name) = node.child_by_field_name("name") {
                        let name = &source[name.byte_range()];
                        *definitions.entry(name.to_owned()).or_insert(0) += 1;
                        if calls::definition_is_inert(
                            node,
                            source,
                            result.annotations_postponed(&file),
                        ) {
                            opaque_bindings.remove(name);
                        }
                        if !names.insert(name.to_owned()) {
                            result.cutoffs.insert("ambiguous-module-binding".into());
                        }
                    }
                    continue;
                }
                for name in opaque_names(node, source) {
                    opaque_bindings.insert(name.to_owned());
                    if !names.insert(name.to_owned()) {
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    }
                }
                if !matches!(
                    node.kind(),
                    "import_statement" | "import_from_statement" | "future_import_statement"
                ) {
                    continue;
                }
                let Some(imports) = imports(node, source, file.strip_prefix(&project.root)?) else {
                    result.cutoffs.insert("unsupported-import-form".into());
                    continue;
                };
                let mut imports: VecDeque<_> = imports
                    .into_iter()
                    .map(|(alias, binding)| (alias, binding, None))
                    .collect();
                while let Some((alias, mut binding, star)) = imports.pop_front() {
                    import_count += 1;
                    if import_count > IMPORT_LIMIT {
                        result.cutoffs.insert("import-lookup-budget".into());
                        break;
                    }
                    if alias != "*" && !names.insert(alias.clone()) {
                        binding.ambiguous = true;
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    }
                    if binding.module.split('.').skip(1).any(special_attribute)
                        || binding.member.as_deref().is_some_and(special_attribute)
                    {
                        result.cutoffs.insert("special-module-attributes".into());
                    }
                    let resolution = resolve_module(project, &binding.module)?;
                    binding.future = future_nodes.contains(&node.start_byte())
                        && resolution
                            .candidates
                            .values()
                            .all(|value| value["status"] == "missing");
                    if binding.future {
                        result
                            .future_nodes
                            .entry(file.clone())
                            .or_default()
                            .insert(node.start_byte());
                    }
                    let mut exports = None;
                    if alias == "*" {
                        let declared = if resolution.admitted {
                            resolution
                                .target
                                .as_ref()
                                .map(|target| export_names(project, target))
                                .transpose()?
                                .flatten()
                        } else {
                            None
                        };
                        if declared.as_ref().is_some_and(|names| {
                            names.len() + import_count + imports.len() > IMPORT_LIMIT
                        }) {
                            result.cutoffs.insert("import-lookup-budget".into());
                        }
                        if let Some(selected) = declared.filter(|names| {
                            names.len() + import_count + imports.len() <= IMPORT_LIMIT
                        }) {
                            let index = lookups.len();
                            for name in selected.iter().rev() {
                                imports.push_front((
                                    name.clone(),
                                    Binding {
                                        module: binding.module.clone(),
                                        member: Some(name.clone()),
                                        prefix: name.clone(),
                                        target: None,
                                        ambiguous: false,
                                        future: false,
                                    },
                                    Some(index),
                                ));
                            }
                            exports = Some(selected);
                        } else {
                            result.cutoffs.insert("star-exports-unchecked".into());
                        }
                    }
                    let mut lookup = serde_json::to_value(&resolution)?;
                    lookup["importer"] = json!(file.strip_prefix(&project.root)?);
                    lookup["alias"] = json!(alias);
                    lookup["member"] = json!(binding.member);
                    lookup["prefix"] = json!(binding.prefix);
                    lookup["star"] = json!(star);
                    lookup["exports"] = json!(exports);
                    lookup["future"] = json!(binding.future);
                    let submodule = if let (Some(target), Some(member)) =
                        (&resolution.target, &binding.member)
                    {
                        let skip = (project.root.join(target) == file).then_some(node.start_byte());
                        if (resolution.namespaces.contains(target)
                            || target.as_path()
                                == Path::new(&format!(
                                    "{}/__init__.py",
                                    binding.module.replace('.', "/")
                                )))
                            && !special_attribute(member)
                            && (resolution.namespaces.contains(target)
                                || !declares_member(project, target, member, skip)?)
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
                            result.retain_namespaces(&project.root, &dependency.namespaces);
                            for path in dependency.packages.iter().chain(&dependency.target) {
                                if dependency.namespaces.contains(path) {
                                    continue;
                                }
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
                    } else if !binding.future {
                        result
                            .cutoffs
                            .insert(format!("unresolved-local-import:{}", selected.module));
                    }
                    lookups.push(lookup);
                    if alias != "*" {
                        bindings.insert(alias, binding);
                    }
                }
            }
            if names.iter().map(String::as_str).any(|name| {
                special_attribute(name)
                    && !(name == "__all__" && declared_exports.is_some())
                    && !(literals.contains(name) && calls::metadata_name(name))
            }) {
                result.cutoffs.insert("special-module-attributes".into());
            }
            files.insert(file.strip_prefix(&project.root)?.to_owned(), hash(source)?);
            result.bindings.insert(file.clone(), bindings);
            result.definitions.insert(file.clone(), definitions);
            result.literals.insert(file.clone(), literals);
            result.opaque_bindings.insert(file.clone(), opaque_bindings);
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
            let (status, chain, module) = if lookup["future"] == true {
                ("future", Vec::new(), None)
            } else if !lookup["submodule"].is_null() {
                if let Some(path) =
                    fallback.filter(|path| result.known_module(&project.root.join(path)))
                {
                    ("module", Vec::new(), Some(path))
                } else {
                    ("unavailable", Vec::new(), None)
                }
            } else {
                match (target, lookup["member"].as_str()) {
                    (Some(file), Some(member)) => result.member_chain(&file, member),
                    (Some(file), None) if result.known_module(&file) => (
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
            if !matches!(status, "function" | "module" | "future") {
                result.cutoffs.insert(format!(
                    "missing-or-ambiguous-import-member:{}",
                    lookup["alias"].as_str().unwrap()
                ));
            }
        }
        let namespaces: Vec<_> = result
            .namespaces
            .iter()
            .map(|path| path.strip_prefix(&project.root).unwrap())
            .collect();
        result.inputs = json!({"schema":"fr-flow-modules", "entry":entry_resolution,"files":files,"namespaces":namespaces,"lookups":lookups,
            "complete":result.cutoffs.is_empty(),"cutoffs":result.cutoffs,
            "policy":"static single-root packages and .py modules; acyclic re-exports, literal __all__ star imports and package child lookup. Correctly placed annotations directives use standard CPython __future__ only when all local candidates are missing. No arbitrary initialization, merged namespaces, default star exports, implicit child traversal, other native modules, search paths or import hooks",
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
        if self
            .literals
            .get(file)
            .is_some_and(|names| names.contains(base))
            || self
                .opaque_bindings
                .get(file)
                .is_some_and(|names| names.contains(base))
        {
            return String::new();
        }
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
