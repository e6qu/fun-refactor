use super::*;
use sha2::{Digest, Sha256};

const STANDARD_MODULES: &str = include_str!("python_standard_modules.txt");

pub(super) fn is_module_name(name: &str) -> bool {
    let first = name.split('.').next().unwrap_or_default();
    STANDARD_MODULES.lines().any(|module| module == first)
}

pub(super) fn members(module: &str) -> Option<&'static [&'static str]> {
    Some(match module {
        "builtins" => &[
            "abs",
            "int",
            "float",
            "bool",
            "round",
            "min",
            "max",
            "ValueError",
            "TypeError",
        ],
        "math" => &[
            "ceil", "floor", "trunc", "sqrt", "fabs", "isfinite", "isinf", "isnan", "inf", "nan",
            "pi", "e", "tau",
        ],
        "bisect" => &[
            "bisect",
            "bisect_left",
            "bisect_right",
            "insort",
            "insort_left",
            "insort_right",
        ],
        "binascii" => &[
            "Error",
            "Incomplete",
            "hexlify",
            "unhexlify",
            "b2a_hex",
            "a2b_hex",
            "b2a_base64",
            "a2b_base64",
            "crc32",
            "crc_hqx",
        ],
        "datetime" => &[
            "date",
            "datetime",
            "time",
            "timedelta",
            "timezone",
            "tzinfo",
            "MINYEAR",
            "MAXYEAR",
            "UTC",
        ],
        "sys" => &[
            "version_info",
            "version",
            "platform",
            "maxsize",
            "implementation",
            "modules",
        ],
        _ => return None,
    })
}

pub(super) fn contract(selected: &BTreeSet<String>) -> Result<Value> {
    if selected.is_empty() {
        return Ok(Value::Null);
    }
    let mut modules = BTreeMap::new();
    for module in selected {
        modules.insert(
            module,
            members(module)
                .with_context(|| format!("unsupported standard-library model: {module}"))?,
        );
    }
    Ok(
        json!({"schema":"fr-python-standard-library", "modules":modules,
        "runtime":"CPython 3.14.8",
        "namespace_digest":hex::encode(Sha256::digest(STANDARD_MODULES.as_bytes())),
        "imports":"Explicit selected standard modules; competing local source, package, stub, directory, bytecode and extension candidates must be absent, including transitive standard-library names.",
        "calls":"Scalar abs/int/float/bool/round and multi-argument min/max; ValueError/TypeError construction in raise; math scalar functions and datetime.date. Other calls remain unchecked.",
        "boundary":"Standard CPython library and builtins, scalar arguments and immutable dates; no replacement import hooks, monkey patching, overloaded protocols or implicit exception model."}),
    )
}

pub(super) fn shadow_candidates(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let names: BTreeSet<_> = STANDARD_MODULES
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect();
    let mut shadows = BTreeSet::new();
    for path in crate::vfs::read_dir_bounded(root, 4096)? {
        let Some(filename) = path.file_name() else {
            continue;
        };
        let filename = filename.to_string_lossy();
        let stem = [".py", ".pyi", ".pyc"]
            .iter()
            .find_map(|suffix| filename.strip_suffix(suffix))
            .or_else(|| {
                [".so", ".pyd", ".dll"]
                    .iter()
                    .any(|suffix| filename.ends_with(suffix))
                    .then(|| filename.split('.').next())
                    .flatten()
            })
            .unwrap_or(&filename);
        if names.contains(stem) {
            shadows.insert(path.strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(shadows)
}

pub(super) fn arity(name: &str) -> Option<(usize, usize)> {
    let name = name.strip_prefix("builtins.").unwrap_or(name);
    Some(match name {
        "abs" | "math.ceil" | "math.floor" | "math.trunc" | "math.sqrt" | "math.fabs"
        | "math.isfinite" | "math.isinf" | "math.isnan" => (1, 1),
        "int" | "float" | "bool" => (0, 1),
        "round" => (1, 2),
        "min" | "max" => (2, 128),
        "datetime.date" => (3, 3),
        "ValueError" | "TypeError" => (0, 1),
        _ => return None,
    })
}

pub(super) fn numeric_default(node: Node<'_>, source: &str) -> bool {
    let Some(node) = calls::unparenthesized(node) else {
        return false;
    };
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    if node.kind() != "call"
        || function.kind() != "identifier"
        || &source[function.byte_range()] != "float"
    {
        return false;
    }
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return false;
    };
    let values: Vec<_> = arguments
        .named_children(&mut arguments.walk())
        .filter(|node| node.kind() != "comment")
        .collect();
    if values.is_empty() {
        return true;
    }
    if values.len() != 1 {
        return false;
    }
    let Some(value) = calls::unparenthesized(values[0]) else {
        return false;
    };
    if value.kind() != "string" {
        return false;
    }
    matches!(
        &source[value.byte_range()],
        "'inf'"
            | "\"inf\""
            | "'-inf'"
            | "\"-inf\""
            | "'infinity'"
            | "\"infinity\""
            | "'-infinity'"
            | "\"-infinity\""
            | "'nan'"
            | "\"nan\""
    )
}

pub(super) fn imports_valid(modules: &Value) -> bool {
    if modules.get("standard_library") == Some(&Value::Null) {
        return modules["standard_imports"] == json!([])
            && modules["standard_shadowing"] == json!([]);
    }
    let Some(selected) = modules["standard_library"]["modules"].as_object() else {
        return false;
    };
    let names: BTreeSet<_> = selected.keys().cloned().collect();
    if contract(&names).ok().as_ref() != Some(&modules["standard_library"]) {
        return false;
    }
    let Some(imports) = modules["standard_imports"].as_array() else {
        return false;
    };
    if imports.len() > 128 {
        return false;
    }
    let mut seen = BTreeSet::new();
    for import in imports {
        let (Some(module), Some(importer), Some(alias), Some(prefix)) = (
            import["module"].as_str(),
            import["importer"].as_str(),
            import["alias"].as_str(),
            import["prefix"].as_str(),
        ) else {
            return false;
        };
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
        ];
        if import.as_object().is_none_or(|value| {
            value.len() != fields.len() || fields.iter().any(|field| !value.contains_key(*field))
        }) || !names.contains(module)
            || modules["files"].get(importer).is_none()
            || alias.is_empty()
            || alias.starts_with("__") && alias.ends_with("__")
            || !alias.chars().enumerate().all(|(index, c)| {
                c == '_' || c.is_ascii_alphabetic() || index > 0 && c.is_ascii_digit()
            })
            || prefix != alias
            || import["admitted"] != false
            || !import["target"].is_null()
            || import["packages"] != json!([])
            || import["namespaces"] != json!([])
            || import["opaque_candidates"] != json!([])
        {
            return false;
        }
        if !import["member"].is_null()
            && !import["member"].as_str().is_some_and(|member| {
                members(module).is_some_and(|members| members.contains(&member))
            })
        {
            return false;
        }
        let paths = [
            format!("{module}.py"),
            format!("{module}/__init__.py"),
            format!("{module}.pyi"),
            format!("{module}/__init__.pyi"),
            module.into(),
        ];
        let expected: BTreeMap<_, _> = paths
            .into_iter()
            .map(|path| (path, json!({"status":"missing"})))
            .collect();
        if import["candidates"] != json!(expected) {
            return false;
        }
        if modules["complete"] == true && !seen.insert((importer, alias)) {
            return false;
        }
    }
    let Some(shadows) = modules["standard_shadowing"].as_array() else {
        return false;
    };
    if shadows.len() > 4096
        || modules["complete"] == true && !shadows.is_empty()
        || !shadows.is_empty() && !names.iter().any(|name| name != "builtins")
    {
        return false;
    }
    let mut prior: Option<&str> = None;
    for shadow in shadows {
        let Some(shadow) = shadow.as_str() else {
            return false;
        };
        if shadow.is_empty()
            || shadow.contains(['/', '\\'])
            || matches!(shadow, "." | "..")
            || prior.is_some_and(|prior| prior >= shadow)
        {
            return false;
        }
        prior = Some(shadow);
    }
    true
}
