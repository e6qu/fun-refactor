use super::{bounded_text, FeatureOptions, Project, RelationshipOptions};
use crate::edit::{Edit, EditSet};
use crate::lang::Language;
use crate::transpile::ir::{Expr, Item, Record, Type};
use crate::transpile::nextjs::Model;
use anyhow::{ensure, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const FEATURE_FACT_LIMIT: usize = 500;

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Author HTTP modules from one bounded, language-neutral application IR.")]
    Application(ApplicationOptions),
    #[command(about = "Plan or apply one revision-bound framework feature migration.")]
    Feature(Options),
}

#[derive(Args)]
pub struct ApplicationOptions {
    #[arg(
        long,
        conflicts_with = "project",
        required_unless_present = "project",
        help = "Workspace-relative route bundle, application IR, or application report JSON file."
    )]
    pub ir: Option<PathBuf>,
    #[arg(
        long,
        conflicts_with = "ir",
        required_unless_present = "ir",
        help = "Build the application IR from this workspace path or revision-bound project handle."
    )]
    pub project: Option<String>,
    #[arg(long, requires = "project")]
    pub revision: Option<String>,
    #[arg(long, requires = "project")]
    pub feature: Option<String>,
    #[arg(long, value_enum)]
    pub to: crate::application_ir::Adapter,
    #[arg(long, help = "Workspace-relative directory for new generated modules.")]
    pub out: PathBuf,
    #[arg(
        long,
        help = "Mount generated routes in an explicit PATH::APP_SYMBOL target."
    )]
    pub register_with: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Update one explicit PEP 621 or npm manifest owning the destination."
    )]
    pub dependency_manifest: Option<PathBuf>,
    #[arg(
        long,
        value_name = "SPEC",
        requires = "dependency_manifest",
        help = "Exact missing requirement to add to the selected manifest."
    )]
    pub dependency_requirement: Vec<String>,
    #[arg(long = "check", value_delimiter = ',')]
    pub checks: Vec<String>,
    #[arg(
        long,
        help = "Remove one wholly owned source feature after connected target integration."
    )]
    pub cutover: bool,
    #[arg(long, default_value_t = 4096)]
    pub diff_bytes: usize,
    #[arg(long)]
    pub write: bool,
}

#[derive(Args)]
pub struct Options {
    #[arg(help = "Feature ID returned by `fr project features`.")]
    pub feature: String,
    #[arg(long, value_enum, help = "Destination framework.")]
    pub to: Target,
    #[arg(
        long,
        help = "Workspace-relative destination file for FastAPI or directory for Next.js."
    )]
    pub out: PathBuf,
    #[arg(
        long,
        help = "Mount generated FastAPI routes in an explicit PATH::APP_SYMBOL target."
    )]
    pub register_with: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Update one explicit PEP 621 pyproject.toml that owns the FastAPI destination."
    )]
    pub dependency_manifest: Option<PathBuf>,
    #[arg(
        long,
        value_name = "SPEC",
        requires = "dependency_manifest",
        help = "Exact Python requirement to add; repeat for every missing generated import."
    )]
    pub dependency_requirement: Vec<String>,
    #[arg(
        long = "check",
        value_name = "NAME",
        value_delimiter = ',',
        help = "Bind one declared project check to the migration transaction; repeat as needed."
    )]
    pub checks: Vec<String>,
    #[arg(
        long,
        help = "Remove the source route in the same transaction after registration checks."
    )]
    pub cutover: bool,
    #[arg(
        long,
        default_value_t = 4096,
        help = "Maximum UTF-8 diff bytes, from 0 through 65536."
    )]
    pub diff_bytes: usize,
    #[arg(long, help = "Record and apply the migration through source history.")]
    pub write: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    Fastapi,
    Nextjs,
}

impl Target {
    fn name(self) -> &'static str {
        match self {
            Self::Fastapi => "fastapi",
            Self::Nextjs => "nextjs",
        }
    }

    fn is_fastapi(self) -> bool {
        matches!(self, Self::Fastapi)
    }
}

pub struct Plan {
    pub edits: EditSet,
    pub connected_changes: Vec<ConnectedChange>,
    pub source_removal: Option<SourceRemoval>,
    pub required_checks: Option<crate::history::CheckRequirement>,
    pub report: Value,
}

pub struct ConnectedChange {
    pub path: PathBuf,
    pub original: String,
    pub updated: String,
}

pub struct SourceRemoval {
    pub path: PathBuf,
    pub original: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct SchemaField {
    name: String,
    declared_type: Option<String>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct SchemaShape {
    name: String,
    fields: Vec<SchemaField>,
}

impl Plan {
    pub fn set_diff(&mut self, diff: &str, budget: usize) {
        self.report["diff"] = bounded_text(diff, budget);
    }
}

fn destination(root: &Path, out: &Path) -> Result<PathBuf> {
    ensure!(
        !out.as_os_str().is_empty()
            && !out.is_absolute()
            && out
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "migration output must be one normalized workspace-relative path."
    );
    Ok(root.join(out))
}

fn normalized_relative(root: &Path, path: &Path, label: &str) -> Result<PathBuf> {
    ensure!(
        !path.as_os_str().is_empty()
            && !path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "{label} must be one normalized workspace-relative path."
    );
    Ok(root.join(path))
}

fn distribution_name(requirement: &str) -> Option<String> {
    let requirement = requirement.trim();
    let end = requirement
        .char_indices()
        .take_while(|(_, character)| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        .map(|(index, character)| index + character.len_utf8())
        .last()?;
    let name = &requirement[..end];
    if !name
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        || !name
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        || requirement[end..].chars().any(char::is_control)
    {
        return None;
    }
    let mut normalized = String::new();
    let mut separator = false;
    for character in name.chars() {
        if matches!(character, '-' | '_' | '.') {
            if !separator {
                normalized.push('-');
            }
            separator = true;
        } else {
            normalized.push(character.to_ascii_lowercase());
            separator = false;
        }
    }
    Some(normalized)
}

struct DependencyPlan {
    change: Option<ConnectedChange>,
    report: Value,
    note: Option<String>,
}

fn fastapi_dependencies(
    project: &Project<'_>,
    dependency_manifest: Option<&Path>,
    dependency_requirement: &[String],
    out: &Path,
    required: &[String],
) -> Result<DependencyPlan> {
    let Some(selected) = dependency_manifest else {
        return Ok(DependencyPlan {
            change: None,
            report: json!({
                "status": "agent-decision",
                "manifest": null,
                "required": required,
                "declared": [],
                "added": [],
                "basis": "generated-runtime-imports",
            }),
            note: Some(
                "No explicit Python dependency manifest proves the generated runtime imports."
                    .to_owned(),
            ),
        });
    };
    let path = normalized_relative(&project.root, selected, "dependency manifest")?;
    ensure!(
        selected
            .file_name()
            .is_some_and(|name| name == "pyproject.toml"),
        "the dependency manifest must name pyproject.toml."
    );
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(anyhow::Error::from)
        .map_err(|error| error.context("reading the dependency manifest metadata"))?;
    ensure!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "the dependency manifest must be a regular file."
    );
    ensure!(
        path.canonicalize().is_ok_and(|canonical| canonical == path),
        "the dependency manifest path must not traverse symbolic links."
    );
    let owner = path.parent().unwrap_or(&project.root);
    let owns_destination = out.starts_with(owner);
    ensure!(
        owns_destination,
        "the dependency manifest must be an ancestor of the FastAPI destination."
    );
    let original = crate::vfs::read_to_string(&path)
        .map_err(anyhow::Error::from)
        .map_err(|error| error.context("reading the dependency manifest"))?;
    ensure!(
        original.len() as u64 <= project.options.max_file_bytes,
        "the dependency manifest exceeds the project file-size limit."
    );
    let manifest_revision = format!("sha256:{}", super::hash(&original)?);
    let mut document = original
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| anyhow::anyhow!("the dependency manifest is not valid TOML."))?;
    let project_table = document
        .get_mut("project")
        .and_then(toml_edit::Item::as_table_mut)
        .ok_or_else(|| anyhow::anyhow!("the dependency manifest needs one [project] table."))?;
    if !project_table.contains_key("dependencies") {
        project_table.insert(
            "dependencies",
            toml_edit::Item::Value(toml_edit::Value::Array(toml_edit::Array::new())),
        );
    }
    let dependencies = project_table
        .get_mut("dependencies")
        .and_then(toml_edit::Item::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("[project].dependencies must be an array."))?;
    let mut declared = BTreeSet::new();
    for value in dependencies.iter() {
        let name = value.as_str().and_then(distribution_name).ok_or_else(|| {
            anyhow::anyhow!("[project].dependencies must contain valid requirement strings.")
        })?;
        ensure!(
            declared.insert(name),
            "[project].dependencies must name each distribution once."
        );
    }
    let missing = required
        .iter()
        .filter(|name| !declared.contains(*name))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut supplied = BTreeMap::new();
    for requirement in dependency_requirement {
        let name = distribution_name(requirement).ok_or_else(|| {
            anyhow::anyhow!(
                "dependency requirements must start with a valid distribution name and contain no control characters."
            )
        })?;
        ensure!(
            supplied
                .insert(name, requirement.trim().to_owned())
                .is_none(),
            "dependency requirements must provide each missing generated runtime import exactly once."
        );
    }
    let requirements_cover_missing =
        supplied.keys().all(|name| missing.contains(name)) && supplied.len() == missing.len();
    ensure!(
        crate::project::framework_kernel::migration_dependency_edit_automatic(
            true,
            owns_destination,
            true,
            requirements_cover_missing,
        ),
        "dependency requirements must provide each missing generated runtime import exactly once."
    );
    for requirement in supplied.values() {
        dependencies.push(requirement);
    }
    let updated = document.to_string();
    let relative = path
        .strip_prefix(&project.root)
        .unwrap_or(&path)
        .to_path_buf();
    Ok(DependencyPlan {
        change: (updated != original).then_some(ConnectedChange {
            path: path.clone(),
            original,
            updated,
        }),
        report: json!({
            "status": if supplied.is_empty() { "satisfied" } else { "updated" },
            "manifest": relative,
            "manifest_revision": manifest_revision,
            "required": required,
            "declared": declared,
            "added": supplied.values().collect::<Vec<_>>(),
            "basis": "explicit-pep-621-project-dependencies",
        }),
        note: None,
    })
}

struct FastapiRegistration {
    path: PathBuf,
    report: Value,
    note: String,
}

fn registration_selector(root: &Path, selector: &str) -> Result<(PathBuf, String)> {
    let (path, application) = selector
        .rsplit_once("::")
        .ok_or_else(|| anyhow::anyhow!("registration target must use PATH::APP_SYMBOL."))?;
    let path = Path::new(path);
    ensure!(
        !path.as_os_str().is_empty()
            && !path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && path.extension().is_some_and(|extension| extension == "py")
            && super::contracts::simple_name(application),
        "registration target must name one normalized Python PATH::APP_SYMBOL."
    );
    Ok((root.join(path), application.to_owned()))
}

fn python_module_path(out: &Path) -> Result<String> {
    let module_path = out.with_extension("");
    let mut parts = module_path
        .components()
        .map(|part| part.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("FastAPI output path is not UTF-8."))?;
    if parts.last() == Some(&"__init__") {
        parts.pop();
    }
    ensure!(
        !parts.is_empty() && parts.iter().all(|part| super::contracts::simple_name(part)),
        "FastAPI output must have Python identifier path components for registration."
    );
    Ok(parts.join("."))
}

fn top_import_offset(root: tree_sitter::Node<'_>) -> usize {
    let mut offset = 0;
    for node in super::fast_routes::children(root) {
        let module_doc = offset == 0
            && node.kind() == "expression_statement"
            && node
                .named_child(0)
                .is_some_and(|child| child.kind() == "string");
        if module_doc
            || matches!(
                node.kind(),
                "import_statement" | "import_from_statement" | "future_import_statement"
            )
        {
            offset = node.end_byte();
        } else if node.kind() != "comment" {
            break;
        }
    }
    offset
}

fn application_assignment_end(
    root: tree_sitter::Node<'_>,
    source: &str,
    application: &str,
) -> Option<usize> {
    super::fast_routes::children(root)
        .into_iter()
        .filter(|node| node.kind() == "expression_statement")
        .filter_map(|statement| {
            let assignment = statement
                .named_children(&mut statement.walk())
                .find(|node| node.kind() == "assignment")?;
            let left = assignment.child_by_field_name("left")?;
            (left.kind() == "identifier" && super::fast_routes::text(left, source) == application)
                .then_some(statement.end_byte())
        })
        .next()
}

fn fresh_registration_alias(source: &str) -> String {
    let mut alias = "fr_migrated_router".to_owned();
    while source.contains(&alias) {
        alias.push('_');
    }
    alias
}

fn add_fastapi_registration(
    project: &Project<'_>,
    edits: &mut EditSet,
    selector: &str,
    out: &Path,
    source: &Path,
    endpoints: &[(String, String)],
) -> Result<FastapiRegistration> {
    let (path, application) = registration_selector(&project.root, selector)?;
    ensure!(
        path != source && path != project.root.join(out),
        "registration target must be separate from the source and generated route."
    );
    let text = project.sources.get(&path).ok_or_else(|| {
        anyhow::anyhow!("registration target is not a captured project source file.")
    })?;
    let parsed = crate::parse::Parsers::new().parse(Language::Python, text)?;
    ensure!(
        !parsed.has_errors(),
        "registration target does not parse cleanly as Python."
    );
    let application_binding = super::fast_routes::receivers(&parsed, text)
        .get(&application)
        .is_some_and(|router| !router);
    let routes = super::fast_routes::read(&parsed, text);
    let endpoint_conflict = routes.entries.iter().any(|route| {
        endpoints
            .iter()
            .any(|(method, url)| route.endpoint.method == *method && route.endpoint.url == *url)
    });
    ensure!(
        crate::project::framework_kernel::fastapi_registration_automatic(
            true,
            application_binding,
            endpoint_conflict,
        ),
        "registration target must contain one recognized FastAPI application binding and no direct endpoint conflict."
    );
    let assignment_end =
        application_assignment_end(parsed.root(), text, &application).ok_or_else(|| {
            anyhow::anyhow!("recognized FastAPI application assignment is unavailable.")
        })?;
    let import_offset = top_import_offset(parsed.root());
    let module = python_module_path(out)?;
    let alias = fresh_registration_alias(text);
    let import = if import_offset == 0 {
        format!("from {module} import router as {alias}\n")
    } else {
        format!("\nfrom {module} import router as {alias}")
    };
    edits.add(
        path.clone(),
        Edit::new(
            crate::span::Span::new(import_offset, import_offset),
            import,
            "Import the generated FastAPI router.",
        ),
    );
    edits.add(
        path.clone(),
        Edit::new(
            crate::span::Span::new(assignment_end, assignment_end),
            format!("\n{application}.include_router({alias})"),
            "Mount the generated FastAPI router.",
        ),
    );
    let relative = path.strip_prefix(&project.root).unwrap_or(&path);
    Ok(FastapiRegistration {
        path: relative.to_path_buf(),
        report: json!({
            "framework": "fastapi",
            "root": relative.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new(".")),
            "entrypoint": relative,
            "application": application,
            "basis": "explicit-recognized-fastapi-binding-and-direct-route-check",
        }),
        note: "FastAPI registration checks direct routes in the selected application file; included and mounted routers remain unchecked.".to_owned(),
    })
}

fn express_registration_selector(root: &Path, selector: &str) -> Result<(PathBuf, String)> {
    let (path, application) = selector
        .rsplit_once("::")
        .ok_or_else(|| anyhow::anyhow!("registration target must use PATH::APP_SYMBOL."))?;
    let path = Path::new(path);
    ensure!(
        !path.as_os_str().is_empty()
            && !path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("ts" | "tsx")
            )
            && super::contracts::simple_name(application),
        "Express registration must name one normalized TypeScript PATH::APP_SYMBOL."
    );
    Ok((root.join(path), application.to_owned()))
}

fn relative_typescript_module(from: &Path, to: &Path) -> Result<String> {
    let from = from.parent().unwrap_or(Path::new(""));
    let left = from.components().collect::<Vec<_>>();
    let right = to.components().collect::<Vec<_>>();
    let common = left
        .iter()
        .zip(&right)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec!["..".to_owned(); left.len().saturating_sub(common)];
    parts.extend(
        right[common..]
            .iter()
            .map(|part| part.as_os_str().to_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| anyhow::anyhow!("generated Express module path is not UTF-8."))?,
    );
    let last = parts
        .last_mut()
        .ok_or_else(|| anyhow::anyhow!("generated Express module path is empty."))?;
    *last = Path::new(last)
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow::anyhow!("generated Express module path is invalid."))?
        .to_owned();
    let path = parts.join("/");
    Ok(if path.starts_with('.') {
        path
    } else {
        format!("./{path}")
    })
}

fn add_express_registration(
    project: &Project<'_>,
    edits: &mut EditSet,
    selector: &str,
    out: &Path,
    source: &Path,
    endpoints: &[(String, String)],
) -> Result<FastapiRegistration> {
    let (path, application) = express_registration_selector(&project.root, selector)?;
    ensure!(
        path != source && path != project.root.join(out),
        "registration target must be separate from the source and generated route."
    );
    let text = project.sources.get(&path).ok_or_else(|| {
        anyhow::anyhow!("registration target is not a captured project source file.")
    })?;
    ensure!(
        !text.starts_with("#!"),
        "Express registration does not edit a shebang module."
    );
    let language = crate::lang::detect(&path)
        .filter(|language| matches!(language, Language::TypeScript | Language::Tsx))
        .ok_or_else(|| anyhow::anyhow!("Express registration target is not TypeScript."))?;
    let parsed = crate::parse::Parsers::new().parse(language, text)?;
    ensure!(
        !parsed.has_errors(),
        "registration target does not parse cleanly as TypeScript."
    );
    let module = crate::transpile::read_module(language, text, parsed.root())?;
    let recognized = module.items.iter().any(|item| {
        matches!(item, Item::Constant(constant) if constant.name == application
            && matches!(&constant.value, Expr::Call { callee, args }
                if args.is_empty() && matches!(&**callee, Expr::Name(name) if matches!(name.as_str(), "express" | "Router"))))
    });
    let conflicts = crate::transpile::routes::endpoints_of(text, language)
        .map(|(_, existing)| {
            existing.iter().any(|existing| {
                endpoints
                    .iter()
                    .any(|(method, path)| existing.method == *method && existing.url == *path)
            })
        })
        .unwrap_or(false);
    ensure!(
        recognized && !conflicts,
        "registration target must contain one recognized Express app/router binding and no direct endpoint conflict."
    );
    let relative = path
        .strip_prefix(&project.root)
        .unwrap_or(&path)
        .to_path_buf();
    let module = relative_typescript_module(&relative, out)?;
    let alias = fresh_registration_alias(text);
    edits.add(
        path.clone(),
        Edit::new(
            crate::span::Span::new(0, 0),
            format!("import {alias} from {};\n", serde_json::to_string(&module)?),
            "Import the generated Express router.",
        ),
    );
    edits.add(
        path.clone(),
        Edit::new(
            crate::span::Span::new(text.len(), text.len()),
            format!("\n{application}.use({alias});\n"),
            "Mount the generated Express router.",
        ),
    );
    Ok(FastapiRegistration {
        path: relative.to_path_buf(),
        report: json!({"framework": "express", "root": relative.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new(".")),
            "entrypoint": relative, "application": application,
            "basis": "explicit-recognized-express-binding-and-direct-route-check"}),
        note: "Express registration preserves existing route and middleware order by mounting the generated router last.".into(),
    })
}

fn go_registration_selector(root: &Path, selector: &str) -> Result<(PathBuf, String)> {
    let (path, mux) = selector
        .rsplit_once("::")
        .ok_or_else(|| anyhow::anyhow!("registration target must use PATH::MUX_SYMBOL."))?;
    let path = Path::new(path);
    ensure!(
        !path.as_os_str().is_empty()
            && !path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && path.extension().is_some_and(|value| value == "go")
            && super::contracts::simple_name(mux),
        "Go registration must name one normalized PATH::MUX_SYMBOL."
    );
    Ok((root.join(path), mux.to_owned()))
}

fn go_module(project: &Project<'_>, relative: &Path, out: &Path) -> Result<(PathBuf, String)> {
    let mut directory = relative.parent().unwrap_or(Path::new(""));
    loop {
        let manifest = directory.join("go.mod");
        if project.manifests.documents.contains_key(&manifest) {
            let path = project.root.join(&manifest);
            let metadata = std::fs::symlink_metadata(&path)?;
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "go.mod must be a regular file."
            );
            let source = crate::vfs::read_to_string(&path)?;
            let modules = source
                .lines()
                .filter_map(|line| line.trim().strip_prefix("module ").map(str::trim))
                .filter(|name| !name.is_empty() && !name.chars().any(char::is_whitespace))
                .collect::<Vec<_>>();
            ensure!(
                modules.len() == 1,
                "go.mod must contain one valid module directive."
            );
            let suffix = out.strip_prefix(directory).map_err(|_| {
                anyhow::anyhow!("generated Go output must be inside the registration module.")
            })?;
            ensure!(
                !suffix.as_os_str().is_empty(),
                "generated Go output must use a separate package directory."
            );
            let suffix = suffix
                .components()
                .map(|part| part.as_os_str().to_str())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| anyhow::anyhow!("generated Go package path is not UTF-8."))?
                .join("/");
            return Ok((
                directory.to_path_buf(),
                format!("{}/{suffix}", modules[0].trim_end_matches('/')),
            ));
        }
        let Some(parent) = directory.parent() else {
            break;
        };
        directory = parent;
    }
    anyhow::bail!("Go registration requires one captured owning go.mod.")
}

fn add_go_registration(
    project: &Project<'_>,
    edits: &mut EditSet,
    selector: &str,
    out: &Path,
    source: &Path,
    endpoints: &[(String, String)],
) -> Result<FastapiRegistration> {
    let (path, mux) = go_registration_selector(&project.root, selector)?;
    ensure!(
        path != source,
        "registration target must be separate from the IR input."
    );
    let text = project.sources.get(&path).ok_or_else(|| {
        anyhow::anyhow!("registration target is not a captured project source file.")
    })?;
    let parsed = crate::parse::Parsers::new().parse(Language::Go, text)?;
    ensure!(
        !parsed.has_errors(),
        "registration target does not parse cleanly as Go."
    );
    let module = crate::transpile::read_module(Language::Go, text, parsed.root())?;
    let root = parsed.root();
    let package = root
        .named_children(&mut root.walk())
        .find(|node| node.kind() == "package_clause")
        .and_then(|node| text[node.byte_range()].split_whitespace().nth(1))
        .filter(|name| super::contracts::simple_name(name))
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("Go registration target omitted its package name."))?;
    let recognized = module.items.iter().any(|item| {
        matches!(item, Item::Constant(constant) if constant.name == mux
            && matches!(&constant.value, Expr::Call { callee, args } if args.is_empty()
                && matches!(&**callee, Expr::Field { of, name } if name == "NewServeMux" && matches!(&**of, Expr::Name(name) if name == "http"))))
    });
    let conflicts = crate::transpile::routes::endpoints_of(text, Language::Go)
        .map(|(_, existing)| {
            existing.iter().any(|existing| {
                endpoints
                    .iter()
                    .any(|(method, path)| existing.method == *method && existing.url == *path)
            })
        })
        .unwrap_or(false);
    ensure!(recognized && !conflicts, "registration target must contain one package-level http.NewServeMux binding and no direct endpoint conflict.");
    let relative = path.strip_prefix(&project.root).unwrap_or(&path);
    let (_module_root, import) = go_module(project, relative, out)?;
    let directory = relative.parent().unwrap_or(Path::new(""));
    ensure!(
        out != directory,
        "generated Go routes must use a package separate from the registration target."
    );
    let mount = directory.join("fr_application_mount.go");
    let mount_absolute = project.root.join(&mount);
    ensure!(
        !crate::vfs::exists(&mount_absolute) && std::fs::symlink_metadata(&mount_absolute).is_err(),
        "generated Go registration file already exists."
    );
    edits.add(
        mount_absolute.clone(),
        Edit::new(
            crate::span::Span::new(0, 0),
            format!("package {package}\n\nimport frgenerated {}\n\nfunc init() {{\n\t{mux}.Handle(\"/\", frgenerated.Handler())\n}}\n", serde_json::to_string(&import)?),
            "Mount the generated Go HTTP handler.",
        ),
    );
    edits.declare_language(mount_absolute, Language::Go);
    Ok(FastapiRegistration {
        path: mount.clone(),
        report: json!({"framework": "go-net-http", "root": if directory.as_os_str().is_empty() { Path::new(".") } else { directory },
            "entrypoint": mount, "application": mux,
            "basis": "explicit-package-level-serve-mux-and-module-import"}),
        note: "Go registration mounts the generated handler at / after refusing direct endpoint conflicts; broader ServeMux precedence requires project checks.".into(),
    })
}

fn npm_dependencies(
    project: &Project<'_>,
    selected: Option<&Path>,
    requirements: &[String],
    out: &Path,
    package: &str,
) -> Result<DependencyPlan> {
    let Some(selected) = selected else {
        return Ok(DependencyPlan {
            change: None,
            report: json!({"status": "agent-decision", "manifest": null, "required": [package], "declared": [], "added": [], "basis": "generated-runtime-imports"}),
            note: Some(format!(
                "No explicit npm manifest proves the generated {package} import."
            )),
        });
    };
    let path = normalized_relative(&project.root, selected, "dependency manifest")?;
    ensure!(
        selected
            .file_name()
            .is_some_and(|name| name == "package.json"),
        "the npm dependency manifest must name package.json."
    );
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(anyhow::Error::from)
        .map_err(|error| error.context("reading the npm dependency manifest metadata"))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "the npm dependency manifest must be a regular file."
    );
    ensure!(
        out.starts_with(path.parent().unwrap_or(&project.root)),
        "the dependency manifest must be an ancestor of the generated destination."
    );
    let original = project
        .sources
        .get(&path)
        .ok_or_else(|| anyhow::anyhow!("dependency manifest is not a captured project file."))?
        .clone();
    let mut document: Value = serde_json::from_str(&original)
        .map_err(|_| anyhow::anyhow!("the npm dependency manifest is not valid JSON."))?;
    let root = document
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("the npm dependency manifest must be an object."))?;
    let dependencies = root.entry("dependencies").or_insert_with(|| json!({}));
    let dependencies = dependencies
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("package.json dependencies must be an object."))?;
    let declared = dependencies
        .get(package)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    let supplied = requirements
        .iter()
        .map(|requirement| {
            let prefix = format!("{package}@");
            let version = requirement
                .strip_prefix(&prefix)
                .filter(|value| {
                    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
                })
                .ok_or_else(|| {
                    anyhow::anyhow!("npm dependency requirement must use {package}@SPEC.")
                })?;
            Ok(version.to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        declared.is_some() && supplied.is_empty() || declared.is_none() && supplied.len() == 1,
        "dependency requirements must provide the one missing generated npm import exactly once."
    );
    let added = if declared.is_none() {
        dependencies.insert(package.to_owned(), Value::String(supplied[0].clone()));
        vec![format!("{package}@{}", supplied[0])]
    } else {
        Vec::new()
    };
    let updated = format!("{}\n", serde_json::to_string_pretty(&document)?);
    let relative = path
        .strip_prefix(&project.root)
        .unwrap_or(&path)
        .to_path_buf();
    Ok(DependencyPlan {
        change: (updated != original).then_some(ConnectedChange {
            path,
            original,
            updated,
        }),
        report: json!({"status": if added.is_empty() { "satisfied" } else { "updated" }, "manifest": relative,
            "required": [package], "declared": declared.into_iter().collect::<Vec<_>>(), "added": added,
            "basis": "explicit-npm-dependencies"}),
        note: None,
    })
}

fn text_set(values: impl Iterator<Item = String>) -> Vec<String> {
    values.collect::<BTreeSet<_>>().into_iter().collect()
}

fn endpoint_set(items: &[Value], feature: &str) -> Vec<(String, String)> {
    items
        .iter()
        .filter_map(|row| {
            (row["kind"] == "route" && row["parent"] == feature).then(|| {
                Some((
                    row["route"]["method"].as_str()?.to_owned(),
                    row["route"]["url"].as_str()?.to_owned(),
                ))
            })?
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn canonical_type(ty: &Type) -> String {
    match ty {
        Type::Unit => "unit".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Int | Type::Float => "number".to_owned(),
        Type::String => "string".to_owned(),
        Type::List(inner) => format!("list<{}>", canonical_type(inner)),
        Type::Set(inner) => format!("set<{}>", canonical_type(inner)),
        Type::Map(key, value) => {
            format!("map<{},{}>", canonical_type(key), canonical_type(value))
        }
        Type::Optional(inner) => format!("optional<{}>", canonical_type(inner)),
        Type::Tuple(parts) => format!(
            "tuple<{}>",
            parts
                .iter()
                .map(canonical_type)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Type::Named { name, args } if args.is_empty() => format!("named<{name}>"),
        Type::Named { name, args } => format!(
            "named<{name};{}>",
            args.iter()
                .map(canonical_type)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Type::Fn { params, returns } => format!(
            "fn<{};{}>",
            params
                .iter()
                .map(canonical_type)
                .collect::<Vec<_>>()
                .join(","),
            canonical_type(returns)
        ),
    }
}

fn schema_shape<'a>(
    name: &str,
    fields: impl Iterator<Item = (&'a str, &'a Option<Type>)>,
) -> SchemaShape {
    let mut fields = fields
        .map(|(name, ty)| SchemaField {
            name: name.to_owned(),
            declared_type: ty.as_ref().map(canonical_type),
        })
        .collect::<Vec<_>>();
    fields.sort();
    SchemaShape {
        name: name.to_owned(),
        fields,
    }
}

fn model_shapes(models: &[Model]) -> Vec<SchemaShape> {
    let mut shapes = models
        .iter()
        .map(|model| {
            schema_shape(
                &model.name,
                model.fields.iter().map(|(name, ty)| (name.as_str(), ty)),
            )
        })
        .collect::<Vec<_>>();
    shapes.sort();
    shapes.dedup();
    shapes
}

fn record_shapes(records: &[Record]) -> Vec<SchemaShape> {
    let mut shapes = records
        .iter()
        .map(|record| {
            schema_shape(
                &record.name,
                record
                    .fields
                    .iter()
                    .map(|field| (field.name.as_str(), &field.ty)),
            )
        })
        .collect::<Vec<_>>();
    shapes.sort();
    shapes.dedup();
    shapes
}

fn generated_shapes(language: Language, outputs: &[String]) -> Result<Vec<SchemaShape>> {
    let parsers = crate::parse::Parsers::new();
    let mut records = Vec::new();
    for output in outputs {
        let parsed = parsers.parse(language, output)?;
        ensure!(
            !parsed.has_errors(),
            "generated migration output failed independent schema parsing."
        );
        let module = crate::transpile::read_module(language, output, parsed.root())?;
        records.extend(module.items.into_iter().filter_map(|item| match item {
            Item::Record(record) => Some(record),
            _ => None,
        }));
    }
    Ok(record_shapes(&records))
}

fn schema_agreement(expected: &[SchemaShape], generated: &[SchemaShape]) -> bool {
    let keys = |shapes: &[SchemaShape]| {
        shapes
            .iter()
            .map(|shape| serde_json::to_string(shape).expect("schema shape is JSON data"))
            .collect::<Vec<_>>()
    };
    crate::project::framework_kernel::migration_schema_agreement(&keys(expected), &keys(generated))
}

fn automatic_kind(kind: &str) -> bool {
    matches!(
        kind,
        "application"
            | "feature"
            | "route"
            | "handler"
            | "contract-field"
            | "schema-reference"
            | "schema-candidate"
            | "schema"
            | "schema-field"
    )
}

fn fact_disposition(row: &Value) -> &'static str {
    let kind = row["kind"].as_str().unwrap_or("unknown");
    match crate::project::framework_kernel::migration_disposition(
        row["status"] == "gap" || kind.ends_with("-gap"),
        automatic_kind(kind),
    ) {
        0 => "automatic",
        1 => "agent-decision",
        _ => "unsupported",
    }
}

fn nextjs_target_application(project: &Project<'_>, out: &Path) -> Option<Value> {
    let parent = out.parent()?;
    let mut roots = vec![parent];
    if parent.file_name().is_some_and(|name| name == "src") {
        roots.push(parent.parent().unwrap_or(Path::new("")));
    }
    for root in roots {
        let app_path = out.strip_prefix(root).ok()?;
        let app_router_path = app_path == Path::new("app") || app_path == Path::new("src/app");
        if !app_router_path {
            continue;
        }
        let manifest = root.join("package.json");
        let Some(document) = project.manifests.documents.get(&manifest) else {
            continue;
        };
        let declares_next = ["dependencies", "devDependencies"].iter().any(|section| {
            document[*section]["next"]
                .as_str()
                .is_some_and(|version| !version.trim().is_empty())
        });
        if !crate::project::framework_kernel::nextjs_registration_automatic(
            declares_next,
            app_router_path,
        ) {
            return None;
        }
        return Some(json!({
            "framework": "nextjs-app",
            "root": if root.as_os_str().is_empty() { "." } else { root.to_str()? },
            "manifest": manifest,
            "basis": "captured-nextjs-dependency-and-app-router-path",
        }));
    }
    None
}

fn source_has_external_references(project: &Project<'_>, source: &Path) -> bool {
    let Some((_, file)) = project
        .index
        .files()
        .find(|(path, _)| path.as_path() == source)
    else {
        return true;
    };
    file.symbols.iter().any(|symbol| {
        project
            .index
            .references_to(*symbol)
            .into_iter()
            .any(|reference| reference.file != source)
    })
}

fn application_source_wholly_owned(
    project: &Project<'_>,
    source: &Path,
    adapter: crate::application_ir::Adapter,
    feature: crate::application_ir::FeatureKind,
) -> bool {
    let relative = source.strip_prefix(&project.root).unwrap_or(source);
    match feature {
        crate::application_ir::FeatureKind::JsonRoute
        | crate::application_ir::FeatureKind::PathJsonRoute => {
            adapter == crate::application_ir::Adapter::Nextjs
                && matches!(
                    relative.file_name().and_then(|name| name.to_str()),
                    Some("route.ts" | "route.tsx" | "route.js" | "route.jsx")
                )
                && relative.components().any(|part| part.as_os_str() == "app")
        }
        crate::application_ir::FeatureKind::StaticComponent => {
            if !matches!(
                adapter,
                crate::application_ir::Adapter::React | crate::application_ir::Adapter::Nextjs
            ) {
                return false;
            }
            let Some(text) = project.sources.get(source) else {
                return false;
            };
            let Ok(parsed) = crate::parse::Parsers::new().parse(Language::Tsx, text) else {
                return false;
            };
            if parsed.has_errors() {
                return false;
            }
            let root = parsed.root();
            let items = root
                .named_children(&mut root.walk())
                .filter(|node| !node.is_extra())
                .collect::<Vec<_>>();
            items.len() == 1
                && items[0].kind() == "export_statement"
                && text[items[0].byte_range()]
                    .trim_start()
                    .starts_with("export default function ")
        }
    }
}

impl Project<'_> {
    pub fn migrate_application(&self, options: &ApplicationOptions) -> Result<Plan> {
        ensure!(
            options.diff_bytes <= 65536,
            "diff byte limit must be between 0 and 65536."
        );
        ensure!(
            matches!(
                options.to,
                crate::application_ir::Adapter::Fastapi
                    | crate::application_ir::Adapter::Express
                    | crate::application_ir::Adapter::GoNetHttp
            ) || options.register_with.is_none(),
            "explicit registration requires the FastAPI, Express or Go HTTP adapter."
        );
        ensure!(
            options.to != crate::application_ir::Adapter::GoNetHttp
                || options.dependency_manifest.is_none()
                    && options.dependency_requirement.is_empty(),
            "Go standard HTTP requires no external framework dependency edit."
        );
        let out = destination(&self.root, &options.out)?;
        let direct_project = options.project.is_some();
        let (input, bytes, mut value) = if let Some(ir) = options.ir.as_deref() {
            let input = normalized_relative(&self.root, ir, "IR input")?;
            let bytes = self
                .sources
                .get(&input)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "IR input must be a JSON file observed in the project snapshot."
                    )
                })?
                .as_bytes()
                .to_vec();
            let value = serde_json::from_slice(&bytes)?;
            (input, bytes, value)
        } else {
            let target = options.project.as_deref().unwrap_or(".");
            let application = self.application_model(
                target,
                options.revision.as_deref(),
                options.feature.as_deref(),
            )?;
            let value = serde_json::to_value(application)?;
            let bytes = serde_json::to_vec(&value)?;
            (
                self.root.join(".fr-project-application-input"),
                bytes,
                value,
            )
        };
        ensure!(
            bytes.len() <= 4_194_304,
            "application IR input exceeds its byte bound."
        );
        let report_input =
            value.get("schema").and_then(Value::as_str) == Some("fr-application-report-1");
        if report_input {
            let model = value
                .get("model")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("application report omitted its model."))?;
            let digest = super::object_merkle(&model)?;
            ensure!(
                value.get("object_digest").and_then(Value::as_str) == Some(digest.as_str()),
                "application report model does not match its object digest."
            );
            value = model;
        }
        let schema = value.get("schema").and_then(Value::as_str);
        let (routes, components, portable_sources, model, source_kind, manual_boundaries) =
            match schema {
                Some("fr-http-application-1") => {
                    let bundle: crate::application_ir::RouteBundle = serde_json::from_value(value)?;
                    bundle.validate().map_err(anyhow::Error::msg)?;
                    let model = serde_json::to_value(&bundle)?;
                    (
                        bundle.routes,
                        Vec::new(),
                        Vec::new(),
                        model,
                        "route-bundle",
                        0usize,
                    )
                }
                Some(crate::application_ir::SCHEMA) => {
                    let application: crate::application_ir::ApplicationIr =
                        serde_json::from_value(value)?;
                    application.validate().map_err(anyhow::Error::msg)?;
                    fn collect(
                        node: &crate::application_ir::ApplicationNode,
                        routes: &mut Vec<crate::application_ir::HttpRoute>,
                        components: &mut Vec<crate::application_ir::StaticComponent>,
                        source: Option<crate::application_ir::Adapter>,
                        sources: &mut BTreeSet<crate::application_ir::Adapter>,
                        portable_sources: &mut Vec<(
                            PathBuf,
                            crate::application_ir::Adapter,
                            crate::application_ir::FeatureKind,
                        )>,
                        manual: &mut usize,
                    ) {
                        let source = if node.kind == "application" {
                            node.data["application"]["framework"]
                                .as_str()
                                .and_then(crate::application_ir::Adapter::from_framework)
                                .or(source)
                        } else {
                            source
                        };
                        if node.kind == "route" {
                            if let Some(route) = &node.route {
                                routes.push(route.clone());
                                if let Some(source) = node.data["route"]["framework"]
                                    .as_str()
                                    .and_then(crate::application_ir::Adapter::from_framework)
                                {
                                    sources.insert(source);
                                    if let Some(path) = node.source["path"].as_str() {
                                        portable_sources.push((
                                            PathBuf::from(path),
                                            source,
                                            route.feature_kind().unwrap(),
                                        ));
                                    }
                                }
                            } else {
                                *manual += 1;
                            }
                        }
                        if node.kind == "component" {
                            if let Some(component) = &node.component {
                                components.push(component.clone());
                                if let Some(source) = source {
                                    sources.insert(source);
                                    if let Some(path) = node.source["path"].as_str() {
                                        portable_sources.push((
                                            PathBuf::from(path),
                                            source,
                                            crate::application_ir::FeatureKind::StaticComponent,
                                        ));
                                    }
                                }
                            } else {
                                *manual += 1;
                            }
                        }
                        for child in &node.children {
                            collect(
                                child,
                                routes,
                                components,
                                source,
                                sources,
                                portable_sources,
                                manual,
                            );
                        }
                    }
                    let mut routes = Vec::new();
                    let mut components = Vec::new();
                    let mut sources = BTreeSet::new();
                    let mut portable_sources = Vec::new();
                    let mut manual = 0;
                    for node in &application.applications {
                        collect(
                            node,
                            &mut routes,
                            &mut components,
                            None,
                            &mut sources,
                            &mut portable_sources,
                            &mut manual,
                        );
                    }
                    ensure!(
                    !sources.contains(&options.to),
                    "source and target adapters must differ; select a compatible target or narrower feature."
                );
                    if !routes.is_empty() {
                        crate::application_ir::validate_routes(&routes)
                            .map_err(anyhow::Error::msg)?;
                    }
                    ensure!(
                        !routes.is_empty() || !components.is_empty(),
                        "application IR has no portable route or component to migrate."
                    );
                    let model = serde_json::to_value(&application)?;
                    (
                        routes,
                        components,
                        portable_sources,
                        model,
                        if direct_project {
                            "project-snapshot"
                        } else if report_input {
                            "project-application-report"
                        } else {
                            "project-application"
                        },
                        manual,
                    )
                }
                _ => {
                    anyhow::bail!("IR input must use fr-http-application-1 or fr-application-ir-1.")
                }
            };
        let mut outputs = BTreeMap::new();
        if !routes.is_empty() && options.to != crate::application_ir::Adapter::React {
            outputs.extend(
                crate::application_ir::write_routes(&routes, options.to)
                    .map_err(anyhow::Error::msg)?,
            );
        }
        if matches!(
            options.to,
            crate::application_ir::Adapter::React | crate::application_ir::Adapter::Nextjs
        ) && !components.is_empty()
        {
            ensure!(
                components.len() == 1,
                "static frontend migration requires exactly one selected component."
            );
            let (path, source) =
                crate::application_ir::write_static_component(&components[0], options.to)
                    .map_err(anyhow::Error::msg)?;
            ensure!(
                outputs.insert(path, source).is_none(),
                "generated frontend destination overlaps another output."
            );
        }
        ensure!(
            !outputs.is_empty(),
            "target adapter has no compatible portable feature in this application."
        );
        let mut edits = EditSet::new();
        let mut files = Vec::new();
        for (relative, source) in &outputs {
            let path = out.join(relative);
            ensure!(
                !crate::vfs::exists(&path) && std::fs::symlink_metadata(&path).is_err(),
                "generated destination already exists: {}",
                path.display()
            );
            let mut parent = path.parent();
            while let Some(directory) = parent {
                if directory == self.root {
                    break;
                }
                if let Ok(metadata) = std::fs::symlink_metadata(directory) {
                    ensure!(
                        metadata.is_dir() && !metadata.file_type().is_symlink(),
                        "generated destination crosses a symlink or non-directory: {}.",
                        directory.display()
                    );
                }
                parent = directory.parent();
            }
            edits.add(
                path,
                Edit::new(
                    crate::span::Span::new(0, 0),
                    source.clone(),
                    "application IR adapter",
                ),
            );
            files.push(json!({"path": options.out.join(relative), "status": "generated", "syntax": "reparse-strict"}));
        }
        let endpoints = routes
            .iter()
            .map(|route| (route.method.clone(), route.path.clone()))
            .collect::<Vec<_>>();
        let registration = options
            .register_with
            .as_deref()
            .map(|selector| match options.to {
                crate::application_ir::Adapter::Fastapi => add_fastapi_registration(
                    self,
                    &mut edits,
                    selector,
                    &options.out.join("routes.py"),
                    &input,
                    &endpoints,
                ),
                crate::application_ir::Adapter::Express => add_express_registration(
                    self,
                    &mut edits,
                    selector,
                    &options.out.join("routes.ts"),
                    &input,
                    &endpoints,
                ),
                crate::application_ir::Adapter::GoNetHttp => add_go_registration(
                    self,
                    &mut edits,
                    selector,
                    &options.out,
                    &input,
                    &endpoints,
                ),
                _ => unreachable!(),
            })
            .transpose()?;
        let next_application = (options.to == crate::application_ir::Adapter::Nextjs)
            .then(|| nextjs_target_application(self, &options.out))
            .flatten();
        let dependency_plan = match options.to {
            crate::application_ir::Adapter::Fastapi => fastapi_dependencies(
                self,
                options.dependency_manifest.as_deref(),
                &options.dependency_requirement,
                &out.join("routes.py"),
                &["fastapi".to_owned()],
            )?,
            crate::application_ir::Adapter::Nextjs
                if options.dependency_manifest.is_none() && next_application.is_some() =>
            {
                DependencyPlan {
                    change: None,
                    report: json!({"status": "satisfied",
                        "manifest": next_application.as_ref().and_then(|application| application["manifest"].as_str()),
                        "required": ["next"], "declared": ["next"], "added": [],
                        "basis": "captured-nextjs-dependency"}),
                    note: None,
                }
            }
            crate::application_ir::Adapter::Nextjs => npm_dependencies(
                self,
                options.dependency_manifest.as_deref(),
                &options.dependency_requirement,
                &out,
                "next",
            )?,
            crate::application_ir::Adapter::Express => npm_dependencies(
                self,
                options.dependency_manifest.as_deref(),
                &options.dependency_requirement,
                &out.join("routes.ts"),
                "express",
            )?,
            crate::application_ir::Adapter::React => npm_dependencies(
                self,
                options.dependency_manifest.as_deref(),
                &options.dependency_requirement,
                &out.join("App.tsx"),
                "react",
            )?,
            crate::application_ir::Adapter::GoNetHttp => DependencyPlan {
                change: None,
                report: json!({"status": "adapter-owned", "manifest": null, "required": [], "declared": [], "added": [], "basis": "no-connected-dependency-edit"}),
                note: None,
            },
        };
        let target_application = registration
            .as_ref()
            .map(|value| value.report.clone())
            .or(next_application);
        let integration_connected = target_application.is_some();
        let integration_reason = if registration.is_some() {
            "The explicit application mount joins this transaction; existing sources remain preserved."
        } else if integration_connected {
            "The captured Next.js App Router placement owns generated modules; existing sources remain preserved."
        } else {
            "Generated modules require explicit application registration. Existing applications retain their source."
        };
        let selected = crate::checks::select(&self.root, &options.checks)?;
        let required_checks = selected
            .as_ref()
            .map(|selection| crate::history::CheckRequirement {
                configuration_basis: selection.configuration_basis.clone(),
                checks: selection.checks.clone(),
            });
        ensure!(
            !options.cutover || direct_project,
            "--cutover requires --project source ownership evidence."
        );
        ensure!(
            !options.cutover || portable_sources.len() == 1,
            "--cutover requires exactly one portable source feature."
        );
        let source_removal = if options.cutover {
            let (source, adapter, feature) = &portable_sources[0];
            let source = self.root.join(source);
            let owned = application_source_wholly_owned(self, &source, *adapter, *feature);
            let external_references = source_has_external_references(self, &source);
            ensure!(
                owned
                    && crate::project::framework_kernel::migration_cutover_automatic(
                        true,
                        integration_connected,
                        external_references,
                    ),
                "--cutover requires one wholly owned source file, connected target integration and no resolved external source references."
            );
            Some(SourceRemoval {
                path: source.clone(),
                original: self.sources[&source].clone(),
            })
        } else {
            None
        };
        let mut report = self.envelope("migration");
        report.as_object_mut().unwrap().extend(serde_json::from_value::<serde_json::Map<String, Value>>(json!({"schema": "fr-application-migration-1",
                "migration": {"source": "application-ir", "source_kind": source_kind, "target": options.to,
                    "ir_object_digest": super::object_merkle(&model)?, "input_basis": format!("frha1:{}", super::hash(("fr-application-input-1", &bytes))?),
                    "files": files, "endpoints": routes, "components": components, "manual_boundaries": manual_boundaries,
                    "coexistence": {"source_preserved": !options.cutover, "cutover_planned": options.cutover, "cutover_applied": false},
                    "integration": {"status": if integration_connected { "connected" } else { "manual" },
                        "registration": target_application,
                        "dependencies": dependency_plan.report,
                        "reason": integration_reason},
                    "runtime_proved": false,
                    "limitations": ["HTTP portability covers declared JSON responses and path parameters; frontend portability covers bounded literal intrinsic JSX.", "Implicit methods, URL decoding, middleware, errors, dynamic UI behavior and deployment behavior require framework checks."]},
                "checks": selected.as_ref().map(|selection| &selection.checks),
                "diff": "", "applied": false}))?);
        Ok(Plan {
            edits,
            connected_changes: dependency_plan.change.into_iter().collect(),
            source_removal,
            required_checks,
            report,
        })
    }

    pub fn migrate_feature(&self, options: &Options) -> Result<Plan> {
        ensure!(
            options.diff_bytes <= 65_536,
            "diff byte limit must be between 0 and 65536."
        );
        let feature_options = FeatureOptions {
            selection: RelationshipOptions {
                target: ".".to_owned(),
                revision: None,
                limit: FEATURE_FACT_LIMIT,
                cursor: None,
            },
            feature: Some(options.feature.clone()),
        };
        let feature_report = self.features(&feature_options)?;
        ensure!(
            feature_report["page"]["remaining"] == 0,
            "the selected feature exceeds the migration fact limit; narrow it before migrating."
        );
        let items = feature_report["items"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let feature = items
            .iter()
            .find(|row| row["kind"] == "feature" && row["id"] == options.feature)
            .ok_or_else(|| anyhow::anyhow!("selected feature fact is unavailable."))?;
        let application_id = feature["parent"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("selected feature has no application parent."))?;
        let application = items
            .iter()
            .find(|row| row["kind"] == "application" && row["id"] == application_id)
            .ok_or_else(|| anyhow::anyhow!("selected feature application is unavailable."))?;
        let source_framework = application["application"]["framework"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("selected application has no framework."))?;
        let source_fastapi = source_framework == "fastapi";
        ensure!(
            matches!(source_framework, "fastapi" | "nextjs-app")
                && crate::project::framework_kernel::framework_migration_supported(
                    source_fastapi,
                    options.to.is_fastapi(),
                ),
            "the bounded migration supports Next.js to FastAPI and FastAPI to Next.js only."
        );

        let semantic_endpoints = endpoint_set(&items, &options.feature);
        ensure!(
            !semantic_endpoints.is_empty(),
            "the selected feature has no route contract to migrate."
        );
        let source_paths = text_set(items.iter().filter_map(|row| {
            (row["kind"] == "route" && row["parent"] == options.feature)
                .then(|| row["source"]["path"].as_str().map(str::to_owned))?
        }));
        ensure!(
            source_paths.len() == 1,
            "the bounded migration requires every selected route to share one source file."
        );
        let source = self.root.join(&source_paths[0]);
        let out = destination(&self.root, &options.out)?;
        match options.to {
            Target::Fastapi => ensure!(
                options
                    .out
                    .extension()
                    .is_some_and(|extension| extension == "py"),
                "a FastAPI migration output must name a .py file."
            ),
            Target::Nextjs => ensure!(
                options.out.file_name().is_some_and(|name| name == "app"),
                "a Next.js migration output must name the destination app directory."
            ),
        }
        ensure!(
            options.register_with.is_none() || matches!(options.to, Target::Fastapi),
            "--register-with applies only to a FastAPI destination."
        );
        ensure!(
            options.dependency_manifest.is_none() || matches!(options.to, Target::Fastapi),
            "--dependency-manifest applies only to a FastAPI destination."
        );
        let check_selection = crate::checks::select(&self.root, &options.checks)?;
        let required_checks =
            check_selection
                .as_ref()
                .map(|selection| crate::history::CheckRequirement {
                    configuration_basis: selection.configuration_basis.clone(),
                    checks: selection.checks.clone(),
                });
        let check_report = check_selection.as_ref().map_or_else(
            || {
                json!({
                    "status": "agent-decision",
                    "configuration_basis": null,
                    "checks": [],
                    "coverage": [],
                })
            },
            |selection| {
                json!({
                    "status": "bound",
                    "configuration_basis": selection.configuration_basis,
                    "checks": selection.checks,
                    "coverage": selection.coverage,
                })
            },
        );

        let (
            mut edits,
            destinations,
            translated_endpoints,
            source_shapes,
            generated_shapes,
            fidelity,
            translator_notes,
            required_dependencies,
        ) = match options.to {
            Target::Fastapi => {
                let translated = crate::transpile::nextjs::plan_to(&source, Some(&out), false)?;
                let mut required_dependencies = vec!["fastapi".to_owned()];
                if translated.output.contains("from pydantic import") {
                    required_dependencies.push("pydantic".to_owned());
                }
                let source_shapes = model_shapes(&translated.models);
                let generated_shapes =
                    generated_shapes(Language::Python, std::slice::from_ref(&translated.output))?;
                let fidelity = json!({
                    "functions": translated.fidelity.functions,
                    "records": translated.fidelity.records,
                    "carried_verbatim": translated.fidelity.carried_verbatim,
                    "signatures_complete": translated.fidelity.signatures_complete,
                });
                (
                    translated.edits,
                    vec![translated.destination],
                    translated.endpoints,
                    source_shapes,
                    generated_shapes,
                    fidelity,
                    translated.fidelity.notes,
                    required_dependencies,
                )
            }
            Target::Nextjs => {
                let translated = crate::transpile::fastapi::plan_to(&source, Some(&out), false)?;
                let endpoints = translated.endpoints;
                let source_shapes = record_shapes(&translated.models);
                let outputs = translated
                    .routes
                    .iter()
                    .map(|route| route.output.clone())
                    .collect::<Vec<_>>();
                let generated_shapes = generated_shapes(Language::TypeScript, &outputs)?;
                let destinations = translated
                    .routes
                    .iter()
                    .map(|route| route.destination.clone())
                    .collect();
                let fidelity = json!({
                    "functions": translated.fidelity.functions,
                    "records": translated.fidelity.records,
                    "carried_verbatim": translated.fidelity.carried_verbatim,
                    "signatures_complete": translated.fidelity.signatures_complete,
                });
                let mut notes = translated.notes;
                notes.extend(translated.fidelity.notes);
                (
                    translated.edits,
                    destinations,
                    endpoints,
                    source_shapes,
                    generated_shapes,
                    fidelity,
                    notes,
                    vec!["next".to_owned()],
                )
            }
        };

        let translated_endpoints = translated_endpoints
            .into_iter()
            .map(|(method, url)| (method.to_uppercase(), url))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        ensure!(
            translated_endpoints == semantic_endpoints,
            "the file translator and selected feature disagree about the route contract."
        );
        ensure!(
            schema_agreement(&source_shapes, &generated_shapes),
            "the generated destination disagrees with the translated declared schema shapes."
        );
        ensure!(
            destinations.iter().all(|path| path != &source),
            "migration must keep the source route while the destination coexists."
        );
        let registration = options
            .register_with
            .as_deref()
            .map(|selector| {
                add_fastapi_registration(
                    self,
                    &mut edits,
                    selector,
                    &options.out,
                    &source,
                    &semantic_endpoints,
                )
            })
            .transpose()?;
        let dependency_plan = match options.to {
            Target::Fastapi => fastapi_dependencies(
                self,
                options.dependency_manifest.as_deref(),
                &options.dependency_requirement,
                &out,
                &required_dependencies,
            )?,
            Target::Nextjs => DependencyPlan {
                change: None,
                report: json!({
                    "status": "satisfied",
                    "manifest": nextjs_target_application(self, &options.out)
                        .and_then(|application| application["manifest"].as_str().map(str::to_owned)),
                    "required": required_dependencies,
                    "declared": ["next"],
                    "added": [],
                    "basis": "captured-nextjs-dependency",
                }),
                note: None,
            },
        };
        let target_application = match options.to {
            Target::Nextjs => nextjs_target_application(self, &options.out),
            Target::Fastapi => registration
                .as_ref()
                .map(|registration| registration.report.clone()),
        };
        let registration_automatic = target_application.is_some();
        let external_references = source_has_external_references(self, &source);
        ensure!(
            !options.cutover
                || crate::project::framework_kernel::migration_cutover_automatic(
                    true,
                    registration_automatic,
                    external_references,
                ),
            "--cutover requires automatic destination registration and no resolved external source references."
        );
        let source_removal = options.cutover.then(|| SourceRemoval {
            path: source.clone(),
            original: self.sources[&source].clone(),
        });

        let facts = items
            .iter()
            .map(|row| {
                json!({
                    "id": row["id"],
                    "kind": row["kind"],
                    "disposition": fact_disposition(row),
                    "source": row["source"],
                })
            })
            .collect::<Vec<_>>();
        let unsupported = text_set(
            items
                .iter()
                .flat_map(|row| row["gaps"].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .chain(translator_notes)
                .chain(
                    registration
                        .iter()
                        .map(|registration| registration.note.clone()),
                )
                .chain(dependency_plan.note.iter().cloned()),
        );
        let destinations_relative = destinations
            .iter()
            .map(|path| path.strip_prefix(&self.root).unwrap_or(path))
            .collect::<Vec<_>>();
        let migration_id = format!(
            "frfm1:{}",
            &super::hash((
                &self.revision,
                &options.feature,
                options.to.name(),
                &options.out,
                &options.register_with,
                &dependency_plan.report,
                &required_checks,
                options.cutover,
            ))?[..32]
        );
        let endpoints = semantic_endpoints
            .iter()
            .map(|(method, url)| json!({"method": method, "url": url}))
            .collect::<Vec<_>>();
        let mut automatic_steps = vec![json!({
            "order": 1,
            "action": "translate-selected-route-file",
            "validation": ["selected-feature-revision", "semantic-translation-endpoint-agreement", "declared-schema-translation-agreement", "reparse-strict"],
        })];
        let mut agent_decisions = Vec::new();
        if registration_automatic {
            automatic_steps.push(match options.to {
                Target::Nextjs => json!({
                    "order": 2,
                    "action": "register-nextjs-route-by-app-router-placement",
                    "validation": ["captured-nextjs-dependency", "app-or-src-app-destination"],
                }),
                Target::Fastapi => json!({
                    "order": 2,
                    "action": "register-fastapi-router-with-application",
                    "validation": ["explicit-application-target", "recognized-fastapi-binding", "no-direct-endpoint-conflict", "reparse-strict"],
                }),
            });
        } else {
            agent_decisions.push(json!({
                "order": 2,
                "action": "register-destination-with-application",
                "reason": "No captured target application proves automatic runtime registration.",
            }));
        }
        if dependency_plan.change.is_some() {
            automatic_steps.push(json!({
                "order": 3,
                "action": "update-python-project-dependencies",
                "validation": ["explicit-pep-621-manifest", "destination-owned-by-manifest", "generated-runtime-imports-covered", "toml-reparse"],
            }));
        } else if dependency_plan.report["status"] == "agent-decision" {
            agent_decisions.push(json!({
                "order": 3,
                "action": "declare-generated-runtime-dependencies",
                "reason": "No explicit Python dependency manifest proves the generated runtime imports.",
            }));
        }
        if check_selection.is_some() {
            automatic_steps.push(json!({
                "order": 4,
                "action": "bind-declared-project-checks",
                "validation": ["captured-check-configuration", "unique-declared-checks", "configuration-basis"],
            }));
        } else {
            agent_decisions.push(json!({
                "order": 4,
                "action": "select-project-checks",
                "reason": "No declared project checks bind to the migration transaction.",
            }));
        }
        if options.cutover {
            automatic_steps.push(json!({
                "order": 5,
                "action": "remove-source-route-after-explicit-cutover",
                "validation": ["explicit-cutover", "automatic-destination-registration", "no-resolved-external-source-references", "source-snapshot"],
            }));
        } else {
            agent_decisions.push(json!({
                "order": 5,
                "action": "run-independent-checks-then-cut-over",
                "reason": "Cutover remains explicit after source-bound project checks pass.",
            }));
        }
        let connected_files = registration
            .iter()
            .map(|registration| registration.path.clone())
            .chain(dependency_plan.change.iter().map(|change| {
                change
                    .path
                    .strip_prefix(&self.root)
                    .unwrap_or(&change.path)
                    .to_path_buf()
            }))
            .collect::<BTreeSet<_>>();
        let mut report = self.envelope("migration");
        report["migration"] = json!({
            "id": migration_id,
            "feature": options.feature,
            "source_framework": source_framework,
            "target_framework": options.to,
            "source_files": source_paths,
            "destination_files": destinations_relative,
            "connected_files": connected_files,
            "target_application": target_application,
            "dependencies": dependency_plan.report,
            "verification": check_report,
            "coexistence": {
                "source_retained": !options.cutover,
                "destination_added": true,
                "destination_registration": if registration_automatic { "automatic" } else { "agent-decision" },
                "cutover_planned": options.cutover,
                "cutover_applied": false,
                "resolved_external_source_references": external_references,
            },
        });
        report["contract"] = json!({
            "endpoints": endpoints,
            "semantic_translation_agreement": true,
            "declared_schemas": {
                "status": if source_shapes.is_empty() { "not-observed" } else { "agreed" },
                "source": source_shapes,
                "generated": generated_shapes,
                "translation_agreement": true,
                "wire_schema_agreement": "unverified",
            },
        });
        report["steps"] = json!({
            "automatic": automatic_steps,
            "agent_decisions": agent_decisions,
            "unsupported": unsupported,
        });
        report["facts"] = json!(facts);
        report["translation"] = fidelity;
        report["scope"] = json!("One route-centered feature whose methods share one source file. Registered destinations may use an explicit source-removal cutover. Runtime checks and unresolved project connections remain reviewed evidence.");
        Ok(Plan {
            edits,
            connected_changes: dependency_plan.change.into_iter().collect(),
            source_removal,
            required_checks,
            report,
        })
    }
}
