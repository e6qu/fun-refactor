use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Function {
    name: String,
    path: PathBuf,
    span: Span,
    defaults: calls::CapturedDefaults,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Statement {
    path: PathBuf,
    span: Span,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: String,
    inputs: Value,
    input_digest: String,
    functions: Vec<Function>,
    function_aliases: BTreeMap<String, (String, usize)>,
    inert_bindings: BTreeSet<String>,
    dictionary_statements: Vec<Statement>,
    dictionary_omitted: usize,
}

impl Project<'_> {
    pub(super) fn startup_inputs(
        &self,
        file: &Path,
        modules: Option<&modules::Modules>,
    ) -> Result<Value> {
        let configuration: BTreeMap<_, _> = self.manifests.snapshots.iter()
            .map(|(path, value)| (path.strip_prefix(&self.root).unwrap_or(path), json!(value)))
            .chain(self.lockfiles.snapshots.iter()
                .map(|(path, value)| (path.strip_prefix(&self.root).unwrap_or(path), json!(value))))
            .collect();
        Ok(json!({"analyzer":super::super::flow_cache::analyzer_identity()?,
            "source":{"path":file.strip_prefix(&self.root)?, "digest":hash(&self.sources[file])?},
            "configuration":hash(configuration)?, "modules":modules.map(|modules| &modules.inputs)}))
    }

    pub(super) fn save_startup(&self, inputs: Value, startup: &Startup<'_, '_>) -> Result<Value> {
        ensure!(startup.initialization.by_file.is_empty() && startup.ambiguous.is_empty(),
            "only checked startup enters saved work.");
        let functions = startup.functions.iter().map(|(name, node)| {
            let (file, _) = startup.contexts[name];
            Ok(Function {name: name.clone(), path: file.strip_prefix(&self.root)?.into(),
                span: Span::from(*node), defaults: startup.captured_defaults[&(file.into(), node.start_byte())].clone()})
        }).collect::<Result<Vec<_>>>()?;
        let dictionary_statements = startup.dictionary_statements.iter().map(|site| Statement {
            path: site.path.clone().into(), span: site.location.span,
        }).collect();
        Ok(serde_json::to_value(Record {schema: "fr-python-startup".into(),
            input_digest: hash(&inputs)?, inputs, functions,
            function_aliases: startup.function_aliases.clone(),
            inert_bindings: startup.inert_bindings.clone(), dictionary_statements,
            dictionary_omitted: startup.dictionary_omitted})?)
    }

    pub(super) fn restore_startup<'a, 't>(
        &'a self,
        path: &Path,
        digest: &str,
        inputs: &Value,
        file: &'a Path,
        parsed: &'t Parsed,
        modules: Option<&'t modules::Modules>,
    ) -> Result<Option<Startup<'a, 't>>>
    where 't: 'a {
        ensure!(std::fs::metadata(path)?.len() <= 1_048_576,
            "saved startup exceeds 1 MiB.");
        let value: Value = serde_json::from_slice(&crate::vfs::read(path)?)?;
        ensure!(hash(&value)? == digest, "saved startup digest mismatch.");
        let record: Record = serde_json::from_value(value)?;
        ensure!(record.schema == "fr-python-startup" && record.input_digest == hash(&record.inputs)?,
            "saved startup has inconsistent identities.");
        if record.inputs != *inputs {
            return Ok(None);
        }
        ensure!(modules.is_none_or(|modules| modules.cutoffs.is_empty()),
            "saved startup requires checked imports.");
        ensure!(record.dictionary_statements.len() <= 16
            && (record.dictionary_omitted == 0 || record.dictionary_statements.len() == 16)
            && record.function_aliases.len() <= 128,
            "saved startup exceeds disclosure limits.");
        let units: BTreeMap<_, _> = match modules {
            Some(modules) => modules.parsed.iter().map(|(path, parsed)| (path.as_path(), parsed)).collect(),
            None => BTreeMap::from([(file, parsed)]),
        };
        let mut startup = Startup {functions: BTreeMap::new(), contexts: BTreeMap::new(),
            captured_defaults: BTreeMap::new(), function_aliases: record.function_aliases,
            ambiguous: BTreeSet::new(), inert_bindings: record.inert_bindings,
            initialization: Initialization::default(), dictionary_statements: Vec::new(),
            dictionary_omitted: record.dictionary_omitted};
        for function in record.functions {
            ensure!(relative(&function.path), "saved function path escapes the workspace.");
            let absolute = self.root.join(&function.path);
            let (file, parsed) = units.get_key_value(absolute.as_path())
                .context("saved function is outside checked files")?;
            let source = self.sources[*file].as_str();
            let node = parsed.root().named_children(&mut parsed.root().walk())
                .find(|node| node.kind() == "function_definition" && Span::from(*node) == function.span)
                .context("saved function does not match a top-level declaration")?;
            let name_node = node.child_by_field_name("name").context("saved function has no name")?;
            let name = &source[name_node.byte_range()];
            let expected = modules.map_or_else(|| name.to_owned(), |modules| modules.function_name(file, name));
            ensure!(expected == function.name && !startup.functions.contains_key(&function.name),
                "saved function has a duplicate or different name.");
            startup.captured_defaults.insert(((*file).into(), node.start_byte()), function.defaults);
            startup.contexts.insert(function.name.clone(), (*file, source));
            startup.functions.insert(function.name, node);
        }
        let expected: usize = units.values().map(|parsed| parsed.root()
            .named_children(&mut parsed.root().walk()).filter(|node| node.kind() == "function_definition").count()).sum();
        ensure!(expected == startup.functions.len(), "saved startup omits a function declaration.");
        ensure!(startup.function_aliases.values().all(|(name, depth)|
            startup.functions.contains_key(name) && (1..=15).contains(depth)),
            "saved startup has an invalid function alias.");
        let mut previous = None;
        for statement in record.dictionary_statements {
            ensure!(relative(&statement.path), "saved statement path escapes the workspace.");
            let absolute = self.root.join(&statement.path);
            let parsed = units.get(absolute.as_path()).context("saved statement is outside checked files")?;
            ensure!(parsed.root().named_children(&mut parsed.root().walk())
                .any(|node| Span::from(node) == statement.span),
                "saved statement does not match a top-level statement.");
            let key = (statement.path, statement.span);
            ensure!(previous.as_ref().is_none_or(|previous| previous < &key),
                "saved statements must be unique and ordered.");
            startup.dictionary_statements.push(self.occurrence(&absolute, key.1, "checked-dictionary-startup")?);
            previous = Some(key);
        }
        Ok(Some(startup))
    }
}

fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components()
        .all(|part| matches!(part, std::path::Component::Normal(_)))
}
