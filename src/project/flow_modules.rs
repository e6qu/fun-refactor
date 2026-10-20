use super::*;
use crate::parse::Parsed;
use std::path::{Path, PathBuf};

#[path = "flow_import_order.rs"]
mod order;

const MODULE_LIMIT: usize = 16;
const BYTE_LIMIT: usize = 262_144;
const IMPORT_LIMIT: usize = 128;
const POLICY: &str = "Single-root modules/packages; local from-import names share a provider search. Child searches use the same budget. Explicit standard model bindings each use one lookup and require absent local candidates. Proved missing sibling imports may select literal metadata handlers. Unused plain module attribute hooks defer; required missing-member hooks stay unchecked. Cycles require available names in execution order. Supported future directives need absent local __future__ candidates. Imported non-entry modules may omit one exact main guard without other __name__ uses or alternatives. Dynamic imports, merged namespaces and search paths stay unchecked.";

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
    standard_library: BTreeSet<String>,
    standard_bindings: BTreeMap<PathBuf, BTreeMap<String, Binding>>,
    pub function_aliases: BTreeMap<PathBuf, BTreeMap<usize, String>>,
    pub value_aliases: BTreeMap<PathBuf, BTreeMap<usize, String>>,
    import_names: BTreeMap<PathBuf, BTreeMap<usize, BTreeSet<String>>>,
    definitions: BTreeMap<PathBuf, BTreeMap<String, usize>>,
    literals: BTreeMap<PathBuf, BTreeSet<String>>,
    values: BTreeMap<PathBuf, BTreeMap<String, (usize, &'static str)>>,
    scalar_values: BTreeSet<(PathBuf, usize)>,
    opaque_bindings: BTreeMap<PathBuf, BTreeSet<String>>,
    future_nodes: BTreeMap<PathBuf, BTreeSet<usize>>,
    future_directives: BTreeMap<PathBuf, BTreeSet<usize>>,
    imported_main_guards: BTreeMap<PathBuf, BTreeSet<usize>>,
    startup_choices: BTreeMap<PathBuf, BTreeMap<usize, usize>>,
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

#[derive(Clone, Serialize)]
struct Resolution {
    module: String,
    candidates: BTreeMap<PathBuf, Value>,
    opaque_candidates: BTreeSet<PathBuf>,
    packages: Vec<PathBuf>,
    namespaces: Vec<PathBuf>,
    target: Option<PathBuf>,
    admitted: bool,
}

fn opaque_candidates(project: &Project<'_>, stem: &str) -> Result<BTreeSet<PathBuf>> {
    let relative = Path::new(stem);
    let parent = relative.parent().unwrap_or(Path::new(""));
    let name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .context("import has no basename")?;
    let directory = project.root.join(parent);
    if !crate::vfs::is_in_memory() {
        let mut ancestor = project.root.clone();
        for component in parent.components() {
            ancestor.push(component);
            match std::fs::symlink_metadata(&ancestor) {
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                    return Ok(BTreeSet::new())
                }
                Ok(_) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
    }
    let entries = match crate::vfs::read_dir_bounded(&directory, 4096) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(error.into()),
    };
    let mut found = BTreeSet::new();
    for entry in entries {
        let Some(filename) = entry.file_name() else {
            continue;
        };
        let filename = filename.to_string_lossy();
        if filename == format!("{name}.pyc")
            || filename.starts_with(&format!("{name}."))
                && [".so", ".pyd", ".dll"]
                    .iter()
                    .any(|suffix| filename.ends_with(suffix))
        {
            found.insert(entry.strip_prefix(&project.root)?.to_path_buf());
            ensure!(
                found.len() <= IMPORT_LIMIT,
                "opaque import candidate budget exceeds 128."
            );
        }
    }
    Ok(found)
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
        opaque_candidates: BTreeSet::new(),
        packages: Vec::new(),
        namespaces: Vec::new(),
        target: None,
        admitted: true,
    };
    for count in 1..=parts.len() {
        let stem = parts[..count].join("/");
        result
            .opaque_candidates
            .extend(opaque_candidates(project, &stem)?);
        ensure!(
            result.opaque_candidates.len() <= IMPORT_LIMIT,
            "opaque import candidate budget exceeds 128."
        );
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
        if status(&directory, "directory") || status(&package, "source") {
            result
                .opaque_candidates
                .extend(opaque_candidates(project, &format!("{stem}/__init__"))?);
            ensure!(
                result.opaque_candidates.len() <= IMPORT_LIMIT,
                "opaque import candidate budget exceeds 128."
            );
        }
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
    result.admitted &= result.opaque_candidates.is_empty();
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

fn imported_main_guards(root: Node<'_>, source: &str) -> BTreeSet<usize> {
    let mut occurrences = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.kind() == "identifier" && &source[node.byte_range()] == "__name__" {
            occurrences.push(node.start_byte());
        }
        pending.extend(node.named_children(&mut node.walk()));
    }
    // Reject every other use, including rebinding, imports and attribute access.
    if occurrences.len() != 1 {
        return BTreeSet::new();
    }
    root.named_children(&mut root.walk())
        .filter(|node| {
            if node.kind() != "if_statement"
                || node
                    .named_children(&mut node.walk())
                    .filter(|child| child.kind() != "comment")
                    .count()
                    != 2
            {
                return false;
            }
            let Some(condition) = node
                .child_by_field_name("condition")
                .and_then(calls::unparenthesized)
            else {
                return false;
            };
            let values: Vec<_> = condition
                .named_children(&mut condition.walk())
                .filter(|child| child.kind() != "comment")
                .collect();
            if condition.kind() != "comparison_operator" || values.len() != 2 {
                return false;
            }
            let (left, right) = (values[0], values[1]);
            left.kind() == "identifier"
                && &source[left.byte_range()] == "__name__"
                && right.kind() == "string"
                && matches!(&source[right.byte_range()], "'__main__'" | "\"__main__\"")
                && source[left.end_byte()..right.start_byte()].trim() == "=="
        })
        .map(|node| node.start_byte())
        .collect()
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
    Ok(export_sequence(project, target)?.map(|names| names.into_iter().collect()))
}

fn has_attribute_hook(project: &Project<'_>, path: &Path) -> Result<bool> {
    let source = &project.sources[&project.root.join(path)];
    if source.len() > BYTE_LIMIT {
        return Ok(true);
    }
    let parsed = Parsers::new().parse(Language::Python, source)?;
    let found = parsed
        .root()
        .named_children(&mut parsed.root().walk())
        .any(|node| {
            node.kind() == "function_definition"
                && node
                    .child_by_field_name("name")
                    .is_some_and(|name| &source[name.byte_range()] == "__getattr__")
        });
    Ok(found)
}

fn export_sequence(project: &Project<'_>, target: &Path) -> Result<Option<Vec<String>>> {
    let Some(source) = project
        .sources
        .get(&project.root.join(target))
        .filter(|s| s.len() <= BYTE_LIMIT)
    else {
        return Ok(None);
    };
    let parsed = Parsers::new().parse(Language::Python, source)?;
    Ok((!parsed.has_errors())
        .then(|| calls::declared_export_sequence(parsed.root(), source))
        .flatten())
}

type MemberValue = (&'static str, Vec<(PathBuf, String)>, Option<PathBuf>);

fn expanded_lookups(modules: &Value) -> Option<Vec<Value>> {
    if modules["policy"] != POLICY
        || modules["limits"]
            != json!({"modules":MODULE_LIMIT,
        "source_bytes":BYTE_LIMIT,"imports":IMPORT_LIMIT,"names_per_import":IMPORT_LIMIT,"module_components":MODULE_LIMIT,"binding_chain":MODULE_LIMIT})
    {
        return None;
    }
    let records = modules["lookups"].as_array()?;
    if records.len() > IMPORT_LIMIT {
        return None;
    }
    let fields = [
        "module",
        "candidates",
        "opaque_candidates",
        "packages",
        "namespaces",
        "target",
        "admitted",
        "importer",
        "alias",
        "member",
        "prefix",
        "exports",
        "future",
        "submodule",
        "resolution",
        "binding_chain",
        "terminal_module",
        "members",
        "names",
    ];
    let member_fields = [
        "name",
        "resolution",
        "binding_chain",
        "submodule",
        "terminal_module",
    ];
    let named_fields = [
        "alias",
        "member",
        "prefix",
        "resolution",
        "binding_chain",
        "submodule",
        "terminal_module",
    ];
    let mut expanded = Vec::new();
    let mut count = records.len();
    for record in records {
        let object = record.as_object()?;
        if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
            return None;
        }
        let members = record["members"].as_array()?;
        let wildcard = record["alias"] == "*";
        let named = !wildcard && !record["member"].is_null();
        if members.len() > IMPORT_LIMIT || !wildcard && !record["exports"].is_null() {
            return None;
        }
        let names = record["names"].as_array();
        if named {
            let names = names?;
            if names.is_empty()
                || names.len() > IMPORT_LIMIT
                || members.len() + 1 != names.len()
                || names[0] != json!([record["alias"], record["member"]])
                || record["prefix"] != record["alias"]
                || names.iter().any(|pair| {
                    pair.as_array().is_none_or(|pair| {
                        pair.len() != 2
                            || pair
                                .iter()
                                .any(|name| name.as_str().is_none_or(|name| !identifier(name)))
                    })
                })
            {
                return None;
            }
        } else if !record["names"].is_null() || !wildcard && !members.is_empty() {
            return None;
        }
        let exports = record["exports"].as_array();
        if wildcard
            && (members.len() != exports.map_or(0, Vec::len)
                || exports.is_none() && !record["exports"].is_null())
        {
            return None;
        }
        let index = expanded.len();
        let mut anchor = record.clone();
        anchor.as_object_mut()?.remove("members");
        anchor.as_object_mut()?.remove("names");
        anchor["star"] = Value::Null;
        count += usize::from(!anchor["submodule"].is_null());
        expanded.push(anchor.clone());
        for (offset, member) in members.iter().enumerate() {
            let object = member.as_object()?;
            let fields = if wildcard {
                member_fields.as_slice()
            } else {
                named_fields.as_slice()
            };
            if object.len() != fields.len()
                || fields.iter().any(|field| !object.contains_key(*field))
            {
                return None;
            }
            if !member["submodule"].is_null() {
                count += 1;
            }
            let mut binding = anchor.clone();
            for field in fields {
                if *field == "name" {
                    continue;
                }
                binding[*field] = member[*field].clone();
            }
            if wildcard {
                let name = member["name"].as_str()?;
                if !identifier(name) || exports?[offset] != name {
                    return None;
                }
                binding["alias"] = json!(name);
                binding["member"] = json!(name);
                binding["prefix"] = json!(name);
                binding["star"] = json!(index);
            } else if names?[offset + 1] != json!([member["alias"], member["member"]])
                || member["prefix"] != member["alias"]
            {
                return None;
            }
            binding["exports"] = Value::Null;
            expanded.push(binding);
        }
    }
    (count <= IMPORT_LIMIT).then_some(expanded)
}

fn aliases_valid(modules: &Value, lookups: &[Value]) -> bool {
    let mut aliases = BTreeMap::new();
    let mut positions = BTreeSet::new();
    for (field, kind) in [("function_aliases", "function"), ("value_aliases", "value")] {
        let Some(items) = modules[field].as_array() else {
            return false;
        };
        if items.len() + aliases.len() > IMPORT_LIMIT {
            return false;
        }
        let mut previous = None;
        for alias in items {
            let (Some(path), Some(name), Some(target), Some(offset)) = (
                alias["path"].as_str(),
                alias["name"].as_str(),
                alias["target"].as_str(),
                alias["offset"].as_u64(),
            ) else {
                return false;
            };
            if alias.as_object().is_none_or(|item| item.len() != 4)
                || modules["files"].get(path).is_none()
                || !identifier(name)
                || special_attribute(name)
                || !identifier(target)
                || offset >= BYTE_LIMIT as u64
                || previous.is_some_and(|previous| previous >= (path, offset))
                || !positions.insert((path, offset))
                || aliases
                    .insert((path, name), (target, offset, kind))
                    .is_some()
            {
                return false;
            }
            previous = Some((path, offset));
        }
    }
    for ((path, name), (_, _, kind)) in &aliases {
        if lookups
            .iter()
            .any(|lookup| lookup["importer"] == *path && lookup["alias"] == *name)
        {
            return false;
        }
        let mut current = (*path, *name);
        let mut seen = BTreeSet::new();
        loop {
            if seen.len() >= MODULE_LIMIT || !seen.insert(current) {
                return false;
            }
            let (target, offset, _) = aliases[&current];
            let next = (current.0, target);
            if let Some((_, next_offset, next_kind)) = aliases.get(&next) {
                if next_offset >= &offset || next_kind != kind {
                    return false;
                }
                current = next;
                continue;
            }
            let imported: Vec<_> = lookups
                .iter()
                .filter(|lookup| lookup["importer"] == current.0 && lookup["alias"] == target)
                .collect();
            if let [imported] = imported.as_slice() {
                if imported["resolution"] != *kind
                    || imported["binding_chain"]
                        .as_array()
                        .is_none_or(|chain| seen.len() + 1 + chain.len() > MODULE_LIMIT)
                {
                    return false;
                }
            } else if !imported.is_empty() || seen.len() + 1 > MODULE_LIMIT {
                return false;
            }
            break;
        }
    }
    for lookup in lookups {
        if !matches!(lookup["resolution"].as_str(), Some("function" | "value")) {
            continue;
        }
        let Some(chain) = lookup["binding_chain"].as_array() else {
            return false;
        };
        for (index, hop) in chain.iter().enumerate() {
            let (Some(path), Some(name)) = (hop["path"].as_str(), hop["name"].as_str()) else {
                return false;
            };
            if let Some((target, _, kind)) = aliases.get(&(path, name)) {
                let Some(next) = chain.get(index + 1) else {
                    return false;
                };
                if lookup["resolution"] != *kind || next["path"] != path || next["name"] != *target
                {
                    return false;
                }
            }
        }
    }
    true
}

pub(super) fn value_bindings_valid(modules: &Value) -> bool {
    let Some(values) = modules["value_bindings"].as_array() else {
        return false;
    };
    let Some(lookups) = expanded_lookups(modules) else {
        return false;
    };
    if !aliases_valid(modules, &lookups) {
        return false;
    }
    if values.len() > IMPORT_LIMIT {
        return false;
    }
    let mut declarations = BTreeSet::new();
    let mut previous = None;
    for value in values {
        let Some(path) = value["path"].as_str() else {
            return false;
        };
        let Some(name) = value["name"].as_str() else {
            return false;
        };
        let Some(offset) = value["offset"].as_u64() else {
            return false;
        };
        if value.as_object().is_none_or(|value| value.len() != 4)
            || modules["files"].get(path).is_none()
            || !identifier(name)
            || special_attribute(name)
            || offset >= BYTE_LIMIT as u64
            || !matches!(
                value["kind"].as_str(),
                Some("literal" | "class" | "instance" | "registry" | "allocation")
            )
            || !declarations.insert((path, name))
            || previous.is_some_and(|previous| previous >= (path, offset))
        {
            return false;
        }
        previous = Some((path, offset));
    }
    let mut referenced = BTreeSet::new();
    for lookup in &lookups {
        if !matches!(lookup["resolution"].as_str(), Some("function" | "value")) {
            continue;
        }
        let Some(terminal) = lookup["binding_chain"]
            .as_array()
            .and_then(|chain| chain.last())
        else {
            return false;
        };
        let Some(path) = terminal["path"].as_str() else {
            return false;
        };
        let Some(name) = terminal["name"].as_str() else {
            return false;
        };
        if declarations.contains(&(path, name)) != (lookup["resolution"] == "value") {
            return false;
        }
        if lookup["resolution"] == "value" {
            referenced.insert((path, name));
        }
    }
    referenced == declarations
}

impl Modules {
    pub(super) fn statement_nodes<'t>(&self, file: &Path, parsed: &'t Parsed) -> Vec<Node<'t>> {
        let mut nodes = Vec::new();
        for node in parsed.root().named_children(&mut parsed.root().walk()) {
            if self
                .imported_main_guards
                .get(file)
                .is_some_and(|nodes| nodes.contains(&node.start_byte()))
            {
                continue;
            }
            if let Some(start) = self
                .startup_choices
                .get(file)
                .and_then(|choices| choices.get(&node.start_byte()))
            {
                let body = node
                    .named_children(&mut node.walk())
                    .filter(|child| child.kind() == "except_clause")
                    .flat_map(|handler| {
                        handler
                            .named_children(&mut handler.walk())
                            .collect::<Vec<_>>()
                    })
                    .find(|child| child.kind() == "block" && child.start_byte() == *start)
                    .expect("checked metadata handler belongs to the unchanged syntax tree");
                nodes.extend(body.named_children(&mut body.walk()));
            } else {
                nodes.push(node);
            }
        }
        nodes
    }

    fn metadata_import<'t>(
        &mut self,
        project: &Project<'_>,
        file: &Path,
        node: Node<'t>,
        source: &str,
        count: &mut usize,
    ) -> Result<Option<(Node<'t>, Resolution, bool)>> {
        if node.kind() != "try_statement" || !calls::initialization_fits(node) {
            return Ok(None);
        }
        let parts: Vec<_> = node
            .named_children(&mut node.walk())
            .filter(|node| node.kind() != "comment")
            .collect();
        if parts.len() != 2 || parts[0].kind() != "block" || parts[1].kind() != "except_clause" {
            return Ok(None);
        }
        let handler: Vec<_> = parts[1]
            .named_children(&mut parts[1].walk())
            .filter(|node| node.kind() != "comment")
            .collect();
        if handler.len() != 2
            || handler[0].kind() != "identifier"
            || &source[handler[0].byte_range()] != "ImportError"
            || handler[1].kind() != "block"
        {
            return Ok(None);
        }
        let body: Vec<_> = parts[0]
            .named_children(&mut parts[0].walk())
            .filter(|node| node.kind() != "comment")
            .collect();
        if body.len() != 1 || body[0].kind() != "import_from_statement" {
            return Ok(None);
        }
        let Some(provider) = body[0].child_by_field_name("module_name") else {
            return Ok(None);
        };
        let provider = &source[provider.byte_range()];
        if !provider.strip_prefix('.').is_some_and(identifier) {
            return Ok(None);
        }
        let statements: Vec<_> = handler[1]
            .named_children(&mut handler[1].walk())
            .filter(|node| node.kind() != "comment")
            .collect();
        if statements.is_empty()
            || statements
                .iter()
                .any(|node| calls::module_metadata(*node, source).is_none())
        {
            return Ok(None);
        }
        let root = node.parent().context("metadata import has no module")?;
        if calls::explicit_builtins(root, source)
            || root.named_children(&mut root.walk()).any(|statement| {
                statement.kind() == "function_definition"
                    && statement
                        .child_by_field_name("name")
                        .is_some_and(|name| &source[name.byte_range()] == "ImportError")
                    || opaque_names(statement, source).contains(&"ImportError")
                    || matches!(
                        statement.kind(),
                        "import_statement" | "import_from_statement"
                    ) && imports(statement, source, file.strip_prefix(&project.root).unwrap())
                        .is_none_or(|items| {
                            items
                                .iter()
                                .any(|(alias, _)| matches!(alias.as_str(), "ImportError" | "*"))
                        })
            })
        {
            return Ok(None);
        }
        let Some(requested) = imports(body[0], source, file.strip_prefix(&project.root)?) else {
            return Ok(None);
        };
        if requested.len() != 1
            || requested[0].1.member.is_none()
            || !calls::metadata_name(&requested[0].0)
        {
            return Ok(None);
        }
        if *count >= IMPORT_LIMIT {
            self.cutoffs.insert("import-lookup-budget".into());
            return Ok(None);
        }
        *count += 1;
        let resolution = resolve_module(project, &requested[0].1.module)?;
        let stem = resolution.module.replace('.', "/");
        let paths = [
            format!("{stem}.py"),
            format!("{stem}/__init__.py"),
            format!("{stem}.pyi"),
            format!("{stem}/__init__.pyi"),
            stem,
        ];
        let selected = !resolution.admitted
            && resolution.target.is_none()
            && resolution.opaque_candidates.is_empty()
            && resolution.namespaces.is_empty()
            && resolution.packages.len() + 1 == resolution.module.split('.').count()
            && paths.iter().all(|path| {
                resolution
                    .candidates
                    .get(Path::new(path))
                    .is_some_and(|value| value["status"] == "missing")
            });
        if !selected {
            self.cutoffs.insert("metadata-import-unchecked".into());
        }
        Ok(Some((handler[1], resolution, selected)))
    }

    pub(super) fn annotations_postponed(&self, file: &Path) -> bool {
        self.future_nodes
            .get(file)
            .is_some_and(|nodes| !nodes.is_empty())
    }

    pub(super) fn future_statement_is_inert(&self, file: &Path, start: usize) -> bool {
        self.future_directives
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
                .values
                .get(&file)
                .is_some_and(|values| values.contains_key(&name))
            {
                return ("value", chain, None);
            }
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

    pub fn load(project: &Project<'_>, entry: &Path, standard: &[String]) -> Result<Self> {
        let standard_library: BTreeSet<_> = standard.iter().cloned().collect();
        ensure!(
            standard_library.len() == standard.len(),
            "standard-library models must be unique."
        );
        let standard_contract = standard_library::contract(&standard_library)?;
        let standard_shadowing = if standard_library.iter().any(|module| module != "builtins") {
            standard_library::shadow_candidates(&project.root)?
        } else {
            BTreeSet::new()
        };
        let name = entry_name(entry.strip_prefix(&project.root)?)
            .context("imported flow requires a root-local Python source entry.")?;
        ensure!(
            !standard_library::is_module_name(&name),
            "entry import may select a standard-library module rather than workspace source."
        );
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
            standard_library,
            standard_bindings: BTreeMap::new(),
            function_aliases: BTreeMap::new(),
            value_aliases: BTreeMap::new(),
            import_names: BTreeMap::new(),
            definitions: BTreeMap::new(),
            literals: BTreeMap::new(),
            values: BTreeMap::new(),
            scalar_values: BTreeSet::new(),
            opaque_bindings: BTreeMap::new(),
            future_nodes: BTreeMap::new(),
            future_directives: BTreeMap::new(),
            imported_main_guards: BTreeMap::new(),
            startup_choices: BTreeMap::new(),
            cutoffs: BTreeSet::new(),
            inputs: Value::Null,
        };
        if !standard_shadowing.is_empty() {
            result
                .cutoffs
                .insert("standard-library-shadowing-unchecked".into());
        }
        result.retain_namespaces(&project.root, &entry_resolution.namespaces);
        if name.split('.').skip(1).any(special_attribute) {
            result.cutoffs.insert("special-module-attributes".into());
        }
        let mut pending = VecDeque::from([entry.to_owned()]);
        let mut queued = BTreeSet::from([entry.to_owned()]);
        let mut files = BTreeMap::new();
        let mut lookups = Vec::new();
        let mut standard_imports = Vec::new();
        let mut metadata_imports = Vec::new();
        let mut star_resolutions: BTreeMap<usize, Resolution> = BTreeMap::new();
        let mut named_members = BTreeMap::new();
        let mut named_declarations = BTreeMap::new();
        let mut edges: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
        let roots: Vec<_> = entry_resolution
            .packages
            .iter()
            .map(|path| project.root.join(path))
            .chain(std::iter::once(entry.to_owned()))
            .collect();
        let mut order = order::Order::default();
        for path in &entry_resolution.packages {
            let path = project.root.join(path);
            if queued.insert(path.clone()) {
                pending.push_back(path.clone());
            }
        }
        let mut bytes = 0;
        let mut import_count = 0;
        let mut alias_count = 0;
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
            let mut values = BTreeMap::new();
            let mut classes = BTreeSet::new();
            let mut opaque_bindings = BTreeSet::new();
            let declared_exports = calls::declared_exports(parsed.root(), source);
            let future_nodes = calls::future_annotation_nodes(parsed.root(), source);
            let directive_nodes =
                calls::future_directive_nodes(parsed.root(), source, calls::FUTURE_DIRECTIVES);
            if file != entry && file.file_name().is_some_and(|name| name != "__main__.py") {
                result
                    .imported_main_guards
                    .insert(file.clone(), imported_main_guards(parsed.root(), source));
            }
            let explicit_builtins = calls::explicit_builtins(parsed.root(), source);
            let mut allocations = initializers::Allocations::default();
            let mut startup = Vec::new();
            for node in parsed.root().named_children(&mut parsed.root().walk()) {
                if let Some((handler, resolution, selected)) =
                    result.metadata_import(project, &file, node, source, &mut import_count)?
                {
                    metadata_imports.push(json!({"importer":file.strip_prefix(&project.root)?, "span":Span::from(node), "handler":Span::from(handler), "lookup":resolution, "selected":selected}));
                    if selected {
                        result
                            .startup_choices
                            .entry(file.clone())
                            .or_default()
                            .insert(node.start_byte(), handler.start_byte());
                    }
                }
            }
            for node in result.statement_nodes(&file, &parsed) {
                let declared = if node.kind() == "function_definition" {
                    node.child_by_field_name("name")
                        .map(|name| vec![&source[name.byte_range()]])
                        .unwrap_or_default()
                } else {
                    opaque_names(node, source)
                };
                if !declared.is_empty() {
                    startup.push(order::Step::Bind(
                        declared.into_iter().map(str::to_owned).collect(),
                    ));
                }
                let allocation = allocations.statement(node, source, |name| {
                    explicit_builtins || names.contains(name)
                });
                if matches!(allocation, Some(initializers::Statement::Population)) {
                    continue;
                }
                let allocated = match allocation {
                    Some(initializers::Statement::Declaration(name)) => Some(name),
                    _ => None,
                };
                if let Some(pairs) = calls::parallel_assignment(node, source).filter(|pairs| {
                    pairs.iter().all(|(_, value)| {
                        calls::collection_value(*value, source, false)
                            || value.kind() == "identifier"
                                && (names.contains(&source[value.byte_range()])
                                    || !explicit_builtins
                                        && calls::builtin_reference(&source[value.byte_range()]))
                    }) && alias_count
                        + pairs
                            .iter()
                            .filter(|(_, value)| {
                                value.kind() == "identifier"
                                    && names.contains(&source[value.byte_range()])
                            })
                            .count()
                        <= IMPORT_LIMIT
                        && pairs
                            .iter()
                            .all(|(name, _)| !names.contains(&source[name.byte_range()]))
                }) {
                    let prior: Vec<_> = pairs
                        .iter()
                        .map(|(_, value)| {
                            value.kind() == "identifier"
                                && names.contains(&source[value.byte_range()])
                        })
                        .collect();
                    for ((name, value), prior) in pairs.into_iter().zip(prior) {
                        let offset = name.start_byte();
                        let name = &source[name.byte_range()];
                        names.insert(name.to_owned());
                        if calls::collection_value(value, source, false) {
                            literals.insert(name.to_owned());
                            values.insert(name.to_owned(), (offset, "literal"));
                            if calls::literal_default(value, source) {
                                result.scalar_values.insert((file.clone(), offset));
                            }
                        } else {
                            let target = &source[value.byte_range()];
                            if prior {
                                alias_count += 1;
                                bindings.insert(
                                    name.to_owned(),
                                    Binding {
                                        module: entry_name(file.strip_prefix(&project.root)?)
                                            .context(
                                                "function alias requires a root-local module",
                                            )?,
                                        member: Some(target.to_owned()),
                                        future: false,
                                        prefix: name.to_owned(),
                                        target: Some(file.strip_prefix(&project.root)?.to_owned()),
                                        ambiguous: false,
                                    },
                                );
                                result
                                    .function_aliases
                                    .entry(file.clone())
                                    .or_default()
                                    .insert(offset, name.to_owned());
                            } else {
                                opaque_bindings.insert(name.to_owned());
                            }
                        }
                    }
                    continue;
                }
                if calls::module_export_list(node, source).is_some() {
                    literals.insert("__all__".to_owned());
                    if !names.insert("__all__".to_owned()) {
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    }
                    continue;
                }
                if let Some(name) = calls::module_literal(node, source)
                    .or_else(|| calls::module_collection(node, source))
                    .or_else(|| calls::module_metadata(node, source))
                {
                    literals.insert(name.to_owned());
                    classes.remove(name);
                    if !names.insert(name.to_owned()) {
                        values.remove(name);
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    } else if !special_attribute(name) {
                        values.insert(name.to_owned(), (node.start_byte(), "literal"));
                        if calls::module_literal(node, source).is_some() {
                            result
                                .scalar_values
                                .insert((file.clone(), node.start_byte()));
                        }
                    }
                    continue;
                }
                let class = calls::module_class(
                    node,
                    source,
                    result.annotations_postponed(&file),
                    |name| explicit_builtins || names.contains(name),
                );
                let registry = (!explicit_builtins)
                    .then(|| calls::module_type_registry(node, source, |name| names.contains(name)))
                    .flatten();
                let value = registry
                    .map(|name| (name, "registry"))
                    .or_else(|| allocated.map(|name| (name, "allocation")))
                    .or_else(|| class.map(|(name, _)| (name, "class")))
                    .or_else(|| {
                        calls::module_instance(node, source, &classes)
                            .map(|name| (name, "instance"))
                    });
                if let Some((name, kind)) = value {
                    classes.remove(name);
                    if !names.insert(name.to_owned()) {
                        values.remove(name);
                        opaque_bindings.insert(name.to_owned());
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    } else {
                        values.insert(name.to_owned(), (node.start_byte(), kind));
                        if class.is_some_and(|(_, constructible)| constructible) {
                            classes.insert(name);
                        }
                    }
                    continue;
                }
                if node.kind() == "function_definition" {
                    if let Some(name) = node.child_by_field_name("name") {
                        let name = &source[name.byte_range()];
                        values.remove(name);
                        classes.remove(name);
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
                if let Some((name, target)) = calls::module_function_alias(node, source) {
                    if names.contains(target) && alias_count < IMPORT_LIMIT {
                        alias_count += 1;
                        let ambiguous = !names.insert(name.to_owned());
                        values.remove(name);
                        classes.remove(name);
                        if ambiguous {
                            result.cutoffs.insert("ambiguous-module-binding".into());
                        }
                        bindings.insert(
                            name.to_owned(),
                            Binding {
                                module: entry_name(file.strip_prefix(&project.root)?)
                                    .context("function alias requires a root-local module")?,
                                member: Some(target.to_owned()),
                                future: false,
                                prefix: name.to_owned(),
                                target: Some(file.strip_prefix(&project.root)?.to_owned()),
                                ambiguous,
                            },
                        );
                        result
                            .function_aliases
                            .entry(file.clone())
                            .or_default()
                            .insert(node.start_byte(), name.to_owned());
                        continue;
                    }
                }
                for name in opaque_names(node, source) {
                    values.remove(name);
                    classes.remove(name);
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
                let named_import = imports.iter().all(|(_, binding)| binding.member.is_some());
                if named_import && imports.len() > IMPORT_LIMIT {
                    result.cutoffs.insert("import-binding-budget".into());
                    continue;
                }
                let requested = json!(imports
                    .iter()
                    .map(|(alias, binding)| (alias, &binding.member))
                    .collect::<Vec<_>>());
                let standard_members_known = imports.iter().all(|(_, binding)| {
                    binding.member.as_ref().is_none_or(|member| {
                        standard_library::members(&binding.module)
                            .is_some_and(|members| members.contains(&member.as_str()))
                    })
                });
                let mut named_provider: Option<(usize, Resolution)> = None;
                let mut startup_imports = Vec::new();
                let mut startup_exports = None;
                let mut imports: VecDeque<_> = imports
                    .into_iter()
                    .map(|(alias, binding)| (alias, binding, None))
                    .collect();
                while let Some((alias, mut binding, star)) = imports.pop_front() {
                    if star.is_none() && named_provider.is_none() {
                        if import_count >= IMPORT_LIMIT {
                            result.cutoffs.insert("import-lookup-budget".into());
                            break;
                        }
                        import_count += 1;
                    }
                    if alias != "*" && !names.insert(alias.clone()) {
                        values.remove(&alias);
                        classes.remove(alias.as_str());
                        binding.ambiguous = true;
                        result.cutoffs.insert("ambiguous-module-binding".into());
                    }
                    if binding.module.split('.').skip(1).any(special_attribute)
                        || binding.member.as_deref().is_some_and(special_attribute)
                    {
                        result.cutoffs.insert("special-module-attributes".into());
                    }
                    let resolution = if let Some(index) = star {
                        star_resolutions[&index].clone()
                    } else if let Some((index, resolution)) = &named_provider {
                        named_members.insert(lookups.len(), *index);
                        resolution.clone()
                    } else {
                        resolve_module(project, &binding.module)?
                    };
                    if named_import && named_provider.is_none() {
                        named_provider = Some((lookups.len(), resolution.clone()));
                        named_declarations.insert(lookups.len(), requested.clone());
                    }
                    let modeled = result.standard_library.contains(&binding.module)
                        && standard_members_known
                        && alias != "*"
                        && resolution
                            .candidates
                            .values()
                            .all(|value| value["status"] == "missing")
                        && resolution.opaque_candidates.is_empty()
                        && binding.member.as_ref().is_none_or(|member| {
                            standard_library::members(&binding.module)
                                .is_some_and(|members| members.contains(&member.as_str()))
                        });
                    if modeled {
                        let mut lookup = serde_json::to_value(&resolution)?;
                        lookup["importer"] = json!(file.strip_prefix(&project.root)?);
                        lookup["alias"] = json!(alias);
                        lookup["member"] = json!(binding.member);
                        lookup["prefix"] = json!(binding.prefix);
                        standard_imports.push(lookup);
                        result
                            .standard_bindings
                            .entry(file.clone())
                            .or_default()
                            .insert(alias.clone(), binding);
                        result
                            .import_names
                            .entry(file.clone())
                            .or_default()
                            .entry(node.start_byte())
                            .or_default()
                            .insert(alias.clone());
                        startup.push(order::Step::Bind(vec![alias]));
                        named_provider = None;
                        named_declarations.remove(&lookups.len());
                        continue;
                    }
                    if matches!(binding.module.split('.').next(), Some("sys" | "builtins")) {
                        result
                            .cutoffs
                            .insert("intrinsic-standard-import-unchecked".into());
                    }
                    if standard_library::is_module_name(&binding.module)
                        && (!resolution.opaque_candidates.is_empty()
                            || resolution
                                .candidates
                                .values()
                                .any(|value| value["status"] != "missing"))
                    {
                        result
                            .cutoffs
                            .insert("standard-library-import-collision".into());
                    }
                    binding.future = directive_nodes.contains(&node.start_byte())
                        && resolution.opaque_candidates.is_empty()
                        && resolution
                            .candidates
                            .values()
                            .all(|value| value["status"] == "missing");
                    if binding.future {
                        result
                            .future_directives
                            .entry(file.clone())
                            .or_default()
                            .insert(node.start_byte());
                    }
                    if binding.future && future_nodes.contains(&node.start_byte()) {
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
                                .map(|target| export_sequence(project, target))
                                .transpose()?
                                .flatten()
                        } else {
                            None
                        };
                        if let Some(sequence) = declared {
                            let selected: BTreeSet<_> = sequence.iter().cloned().collect();
                            startup_exports = Some(sequence);
                            let index = lookups.len();
                            star_resolutions.insert(index, resolution.clone());
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
                    let mut exhausted_child = false;
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
                            if !resolution.namespaces.contains(target)
                                && has_attribute_hook(project, target)?
                            {
                                result
                                    .cutoffs
                                    .insert("import-attribute-hook-unchecked".into());
                            }
                            let child = format!("{}.{}", binding.module, member);
                            if module_name(&child) {
                                if import_count >= IMPORT_LIMIT {
                                    result.cutoffs.insert("import-lookup-budget".into());
                                    exhausted_child = true;
                                    None
                                } else {
                                    import_count += 1;
                                    Some(resolve_module(project, &child)?)
                                }
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
                    lookup["exhausted_child"] = json!(exhausted_child);
                    let selected = submodule.as_ref().unwrap_or(&resolution);
                    let mut dependencies = Vec::new();
                    for dependency in std::iter::once(&resolution).chain(submodule.iter()) {
                        if dependency.admitted {
                            result.retain_namespaces(&project.root, &dependency.namespaces);
                            for path in dependency.packages.iter().chain(&dependency.target) {
                                if dependency.namespaces.contains(path) {
                                    continue;
                                }
                                let absolute = project.root.join(path);
                                edges
                                    .entry(file.clone())
                                    .or_default()
                                    .insert(absolute.clone());
                                dependencies.push(absolute.clone());
                                if queued.insert(absolute.clone()) {
                                    pending.push_back(absolute);
                                }
                            }
                        }
                    }
                    if selected.admitted && !exhausted_child {
                        binding.target = selected.target.clone();
                        if submodule.is_some() {
                            binding.member = None;
                        }
                    } else if !binding.future && !exhausted_child {
                        result
                            .cutoffs
                            .insert(format!("unresolved-local-import:{}", selected.module));
                    }
                    if !binding.future {
                        startup_imports.push(order::Import {
                            dependencies,
                            target: binding.target.as_ref().map(|path| project.root.join(path)),
                            member: if alias == "*" {
                                Some("__all__".to_owned())
                            } else {
                                binding.member.clone()
                            },
                            alias: (alias != "*").then(|| alias.clone()),
                        });
                    }
                    lookups.push(lookup);
                    if alias != "*" {
                        result
                            .import_names
                            .entry(file.clone())
                            .or_default()
                            .entry(node.start_byte())
                            .or_default()
                            .insert(alias.clone());
                        bindings.insert(alias, binding);
                    }
                }
                if let Some(exports) = startup_exports {
                    startup_imports.sort_by_key(|item| {
                        item.alias.as_ref().map_or(0, |alias| {
                            exports
                                .iter()
                                .position(|name| name == alias)
                                .expect("wildcard expansion retains its declared name")
                                + 1
                        })
                    });
                }
                startup.push(order::Step::Imports {
                    items: startup_imports,
                    from_import: node.kind() != "import_statement",
                });
            }
            if names.iter().map(String::as_str).any(|name| {
                special_attribute(name)
                    && !(name == "__all__" && declared_exports.is_some())
                    && !(literals.contains(name) && calls::metadata_name(name))
                    && !(matches!(name, "__getattr__" | "__dir__")
                        && definitions.get(name) == Some(&1)
                        && !bindings.contains_key(name)
                        && !opaque_bindings.contains(name))
            }) {
                result.cutoffs.insert("special-module-attributes".into());
            }
            files.insert(file.strip_prefix(&project.root)?.to_owned(), hash(source)?);
            result.bindings.insert(file.clone(), bindings);
            result.definitions.insert(file.clone(), definitions);
            result.literals.insert(file.clone(), literals);
            result.values.insert(file.clone(), values);
            result.opaque_bindings.insert(file.clone(), opaque_bindings);
            order.steps.insert(file.clone(), startup);
            result.parsed.insert(file, parsed);
        }
        let mut visited = BTreeSet::new();
        if result
            .parsed
            .keys()
            .any(|file| cyclic(file, &edges, &mut BTreeSet::new(), &mut visited))
        {
            match order.check(&roots) {
                order::Check::Complete => (),
                order::Check::UnavailableCycleMember => {
                    result.cutoffs.insert("cyclic-module-initialization".into());
                }
                order::Check::Unknown => {
                    result
                        .cutoffs
                        .insert("import-initialization-order-unchecked".into());
                }
            }
        }
        if result.package_binding_conflicts() {
            result
                .cutoffs
                .insert("package-child-binding-conflict".into());
        }
        let aliases = std::mem::take(&mut result.function_aliases);
        for (file, items) in aliases {
            for (offset, name) in items {
                if result.is_value(&file, &name) {
                    result
                        .value_aliases
                        .entry(file.clone())
                        .or_default()
                        .insert(offset, name);
                } else if result.resolve(&file, &name).is_empty() {
                    result
                        .opaque_bindings
                        .entry(file.clone())
                        .or_default()
                        .insert(name);
                } else {
                    result
                        .function_aliases
                        .entry(file.clone())
                        .or_default()
                        .insert(offset, name);
                }
            }
        }
        for lookup in &mut lookups {
            let target = lookup["target"]
                .as_str()
                .map(|path| project.root.join(path));
            let fallback = lookup["submodule"]["target"].as_str().map(PathBuf::from);
            let exhausted_child = lookup.as_object_mut().unwrap().remove("exhausted_child");
            let (status, chain, module) = if exhausted_child == Some(json!(true)) {
                ("budget", Vec::new(), None)
            } else if lookup["future"] == true {
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
            if !matches!(status, "function" | "value" | "module" | "future") {
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
        let alias_bindings = &result.bindings;
        let mut referenced_values = BTreeMap::new();
        for lookup in &lookups {
            if lookup["resolution"] != "value" {
                continue;
            }
            let terminal = lookup["binding_chain"].as_array().unwrap().last().unwrap();
            let path = terminal["path"].as_str().unwrap();
            let name = terminal["name"].as_str().unwrap();
            let (offset, kind) = result.values[&project.root.join(path)][name];
            referenced_values.insert(
                (path, offset),
                json!({"path":path,"name":name,"offset":offset,"kind":kind}),
            );
        }
        let value_bindings: Vec<_> = referenced_values.into_values().collect();
        ensure!(
            value_bindings.len() <= IMPORT_LIMIT,
            "import value declaration budget exceeds 128"
        );
        let aliases: Vec<_> = result
            .function_aliases
            .iter()
            .flat_map(|(file, aliases)| {
                aliases.iter().map(move |(offset, name)| {
                    json!({
                        "path":file.strip_prefix(&project.root).unwrap(), "name":name,
                        "target":alias_bindings[file][name].member, "offset":offset
                    })
                })
            })
            .collect();
        let value_aliases: Vec<_> = result
            .value_aliases
            .iter()
            .flat_map(|(file, aliases)| {
                aliases.iter().map(move |(offset, name)| {
                    json!({
                        "path":file.strip_prefix(&project.root).unwrap(), "name":name,
                        "target":alias_bindings[file][name].member, "offset":offset
                    })
                })
            })
            .collect();
        let mut compact: Vec<Value> = Vec::new();
        let mut anchors: BTreeMap<usize, usize> = BTreeMap::new();
        for (index, mut lookup) in lookups.into_iter().enumerate() {
            let star = lookup.as_object_mut().unwrap().remove("star").unwrap();
            if let Some(star) = star.as_u64() {
                let anchor = anchors[&(star as usize)];
                let members = compact[anchor]["members"].as_array_mut().unwrap();
                members.push(
                    json!({"name":lookup["alias"], "resolution":lookup["resolution"],
                    "binding_chain":lookup["binding_chain"], "submodule":lookup["submodule"],
                    "terminal_module":lookup["terminal_module"]}),
                );
            } else if let Some(parent) = named_members.get(&index) {
                let anchor = anchors[parent];
                compact[anchor]["members"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({
                    "alias":lookup["alias"], "member":lookup["member"], "prefix":lookup["prefix"],
                    "resolution":lookup["resolution"], "binding_chain":lookup["binding_chain"],
                    "submodule":lookup["submodule"], "terminal_module":lookup["terminal_module"]}));
            } else {
                anchors.insert(index, compact.len());
                lookup["members"] = json!([]);
                lookup["names"] = named_declarations
                    .get(&index)
                    .cloned()
                    .unwrap_or(Value::Null);
                compact.push(lookup);
            }
        }
        metadata_imports.sort_by_key(|record| {
            (
                record["importer"].as_str().unwrap().to_owned(),
                record["span"]["start"].as_u64().unwrap(),
            )
        });
        result.inputs = json!({"schema":"fr-flow-modules", "entry":entry_resolution,"files":files,"namespaces":namespaces,"lookups":compact,
            "metadata_imports":metadata_imports,
            "standard_library":standard_contract,"standard_imports":standard_imports,"standard_shadowing":standard_shadowing,
            "function_aliases":aliases,
            "value_aliases":value_aliases,
            "value_bindings":value_bindings,
            "complete":result.cutoffs.is_empty(),"cutoffs":result.cutoffs,
            "policy":POLICY,
            "limits":{"modules":MODULE_LIMIT,"source_bytes":BYTE_LIMIT,"imports":IMPORT_LIMIT,"names_per_import":IMPORT_LIMIT,"module_components":MODULE_LIMIT,"binding_chain":MODULE_LIMIT}});
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
        if self
            .standard_bindings
            .get(file)
            .and_then(|bindings| bindings.get(name))
            .is_some_and(|binding| !binding.ambiguous && binding.member.is_none())
        {
            return true;
        }
        self.member_chain(file, name).0 == "module"
    }

    pub fn is_value(&self, file: &Path, name: &str) -> bool {
        if self
            .standard_bindings
            .get(file)
            .and_then(|bindings| bindings.get(name))
            .is_some_and(|binding| !binding.ambiguous && binding.member.is_some())
        {
            return true;
        }
        self.member_chain(file, name).0 == "value"
    }

    pub fn is_scalar_value(&self, file: &Path, name: &str) -> bool {
        let (status, chain, _) = self.member_chain(file, name);
        if status != "value" {
            return false;
        }
        let Some((path, name)) = chain.last() else {
            return false;
        };
        let path = self.root.join(path);
        self.values
            .get(&path)
            .and_then(|values| values.get(name))
            .is_some_and(|(offset, kind)| {
                *kind == "literal" && self.scalar_values.contains(&(path.clone(), *offset))
            })
    }

    pub fn is_function(&self, file: &Path, name: &str) -> bool {
        self.member_chain(file, name).0 == "function"
    }

    pub fn names_imported_at<'a>(
        &'a self,
        file: &'a Path,
        start: usize,
    ) -> impl Iterator<Item = &'a str> + 'a {
        self.import_names
            .get(file)
            .and_then(|statements| statements.get(&start))
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    pub fn function_name(&self, file: &Path, name: &str) -> String {
        format!(
            "{}::{name}",
            file.strip_prefix(&self.root)
                .expect("module belongs to workspace")
                .display()
        )
    }

    pub fn standard_enabled(&self, module: &str) -> bool {
        self.standard_library.contains(module)
    }

    pub fn has_binding(&self, file: &Path, name: &str) -> bool {
        self.bindings
            .get(file)
            .is_some_and(|bindings| bindings.contains_key(name))
            || self
                .standard_bindings
                .get(file)
                .is_some_and(|bindings| bindings.contains_key(name))
    }

    pub fn has_imported_name(&self, name: &str) -> bool {
        self.bindings
            .values()
            .any(|bindings| bindings.contains_key(name))
            || self
                .standard_bindings
                .values()
                .any(|bindings| bindings.contains_key(name))
    }

    pub fn standard_names<'a>(&'a self, file: &Path) -> impl Iterator<Item = &'a str> {
        self.standard_bindings
            .get(file)
            .into_iter()
            .flat_map(|bindings| bindings.keys().map(String::as_str))
    }

    pub fn standard_callee(&self, file: &Path, name: &str) -> Option<String> {
        let base = name.split('.').next()?;
        let binding = self.standard_bindings.get(file)?.get(base)?;
        if binding.ambiguous {
            return None;
        }
        let member = match &binding.member {
            Some(member) if name == base => member.as_str(),
            None => name.strip_prefix(&format!("{}.", binding.prefix))?,
            _ => return None,
        };
        standard_library::members(&binding.module)?
            .contains(&member)
            .then(|| format!("{}.{member}", binding.module))
    }
}
