use super::*;
use anyhow::ensure;

#[derive(clap::Args)]
pub struct Options {
    name: String,
    #[arg(long = "in", default_value = ".")]
    scope: String,
    #[arg(long)]
    revision: Option<String>,
    #[arg(
        long,
        help = "Match a case-sensitive literal substring instead of the entire name."
    )]
    contains: bool,
    #[arg(long)]
    locals: bool,
    #[arg(long)]
    signature: bool,
    #[arg(long, help = "Include source slices within a shared page byte budget.")]
    source: bool,
    #[arg(long, default_value_t = 2048, requires = "source")]
    bytes: usize,
    #[arg(long, default_value_t = 12)]
    limit: usize,
    #[arg(long)]
    cursor: Option<String>,
}

#[derive(clap::Args)]
pub struct SelectOptions {
    #[arg(required = true, num_args = 1..=32)]
    names: Vec<String>,
    #[arg(long = "in", default_value = ".")]
    scope: String,
    #[arg(long)]
    revision: Option<String>,
    #[arg(long)]
    locals: bool,
    #[arg(long)]
    signature: bool,
    #[arg(long, help = "Include source slices within a shared page byte budget.")]
    source: bool,
    #[arg(long, default_value_t = 2048, requires = "source")]
    bytes: usize,
    #[arg(long, default_value_t = 24)]
    limit: usize,
    #[arg(long)]
    cursor: Option<String>,
}

impl Project<'_> {
    pub(super) fn find(&self, options: &Options) -> Result<Value> {
        if options.name.is_empty() || options.name.len() > 512 {
            bail!("Choose a nonempty literal name of at most 512 UTF-8 bytes.");
        }
        check_limit(options.limit)?;
        if options.source && !(4..=65536).contains(&options.bytes) {
            bail!("source bytes must be between 4 and 65536.");
        }
        let selected =
            self.target(&self.explicit_handle(&options.scope, options.revision.as_deref())?)?;
        let mut stack = vec![selected];
        let mut matches = Vec::new();
        let mut hidden_locals = 0;
        while let Some(id) = stack.pop() {
            let node = &self.nodes[id];
            if node.symbol.is_some() {
                let matched = if options.contains {
                    node.name.contains(&options.name)
                } else {
                    node.name == options.name
                };
                if matched {
                    if !options.locals && self.local(id) {
                        hidden_locals += 1;
                    } else {
                        matches.push((id, node.parent, 0));
                    }
                }
            }
            stack.extend(node.children.iter().rev());
        }
        let key = format!(
            "frpc1:{}",
            &hash((
                &self.revision,
                "find",
                selected,
                &options.name,
                options.contains,
                options.locals,
                options.signature
            ))?[..32]
        );
        let key = if options.source {
            format!("frpc1:{}", &hash((&key, "source", options.bytes))?[..32])
        } else {
            key
        };
        let (start, end, page) = page(
            matches.len(),
            options.limit,
            options.cursor.as_deref(),
            &key,
        )?;
        let mut fields = vec![
            Field::Handle,
            Field::Parent,
            Field::Kind,
            Field::Name,
            Field::Path,
            Field::Line,
        ];
        if options.signature {
            fields.push(Field::Signature);
        }
        let mut report = self.envelope("find");
        report["root"] = json!(self.handle(selected));
        report["match"] = json!({"name": options.name, "mode": if options.contains { "literal-substring" } else { "exact" }});
        report["columns"] = json!(fields);
        let mut rows = self.rows(&matches[start..end], &fields)?;
        if options.source {
            report["columns"]
                .as_array_mut()
                .unwrap()
                .push(json!("source"));
            let mut remaining = options.bytes;
            for (row, (id, _, _)) in rows.iter_mut().zip(&matches[start..end]) {
                let source = self.source_slice(*id, 0, remaining)?;
                remaining -= source["returned_bytes"].as_u64().unwrap() as usize;
                row.push(source);
            }
            report["source_budget"] = json!({"limit_bytes": options.bytes,
                "returned_bytes": options.bytes - remaining, "scope": "returned page"});
        }
        report["rows"] = json!(rows);
        report["page"] = page;
        report["omitted"] = json!({"matching_locals": hidden_locals});
        Ok(report)
    }

    pub(super) fn select(&self, options: &SelectOptions) -> Result<Value> {
        ensure!(
            options
                .names
                .iter()
                .all(|name| !name.is_empty() && name.len() <= 512),
            "Choose 1 through 32 nonempty literal names of at most 512 UTF-8 bytes each."
        );
        ensure!(
            options.names.iter().map(String::len).sum::<usize>() <= 4096,
            "Selected names exceed the combined 4096-byte limit."
        );
        let unique = options.names.iter().collect::<BTreeSet<_>>();
        ensure!(
            unique.len() == options.names.len(),
            "Selected names must be unique."
        );
        check_limit(options.limit)?;
        if options.source && !(4..=65536).contains(&options.bytes) {
            bail!("source bytes must be between 4 and 65536.");
        }
        let selected =
            self.target(&self.explicit_handle(&options.scope, options.revision.as_deref())?)?;
        let requested = options
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.as_str(), index))
            .collect::<BTreeMap<_, _>>();
        let mut matches = vec![Vec::new(); options.names.len()];
        let mut hidden_locals = vec![0usize; options.names.len()];
        let mut stack = vec![selected];
        while let Some(id) = stack.pop() {
            let node = &self.nodes[id];
            if node.symbol.is_some() {
                if let Some(index) = requested.get(node.name.as_str()).copied() {
                    if !options.locals && self.local(id) {
                        hidden_locals[index] += 1;
                    } else {
                        matches[index].push((id, node.parent, 0));
                    }
                }
            }
            stack.extend(node.children.iter().rev());
        }
        let combined = matches
            .iter()
            .enumerate()
            .flat_map(|(request, rows)| rows.iter().copied().map(move |row| (request, row)))
            .collect::<Vec<_>>();
        let key = format!(
            "frpc1:{}",
            &hash((
                &self.revision,
                "select",
                selected,
                &options.names,
                options.locals,
                options.signature
            ))?[..32]
        );
        let key = if options.source {
            format!("frpc1:{}", &hash((&key, "source", options.bytes))?[..32])
        } else {
            key
        };
        let (start, end, page) = page(
            combined.len(),
            options.limit,
            options.cursor.as_deref(),
            &key,
        )?;
        let mut fields = vec![
            Field::Handle,
            Field::Parent,
            Field::Kind,
            Field::Name,
            Field::Path,
            Field::Line,
        ];
        if options.signature {
            fields.push(Field::Signature);
        }
        let page_matches = &combined[start..end];
        let nodes = page_matches.iter().map(|(_, row)| *row).collect::<Vec<_>>();
        let mut rows = self.rows(&nodes, &fields)?;
        for (row, (request, _)) in rows.iter_mut().zip(page_matches) {
            row.insert(0, json!(&options.names[*request]));
        }
        let mut columns = vec![json!("request")];
        columns.extend(fields.iter().map(|field| json!(field)));
        let mut returned_bytes = 0usize;
        if options.source {
            columns.push(json!("source"));
            let mut remaining = options.bytes;
            for (row, (_, (id, _, _))) in rows.iter_mut().zip(page_matches) {
                let source = self.source_slice(*id, 0, remaining)?;
                let bytes = source["returned_bytes"].as_u64().unwrap() as usize;
                remaining -= bytes;
                returned_bytes += bytes;
                row.push(source);
            }
        }
        let mut returned = vec![0usize; options.names.len()];
        for (request, _) in page_matches {
            returned[*request] += 1;
        }
        let selections = options
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let total = matches[index].len();
                let status = if total > 0 {
                    "matched"
                } else if hidden_locals[index] > 0 {
                    "matching-locals-omitted"
                } else {
                    "no-indexed-match"
                };
                json!({"name": name, "status": status, "total": total,
                    "returned": returned[index], "matching_locals_omitted": hidden_locals[index]})
            })
            .collect::<Vec<_>>();
        let mut report = self.envelope("select");
        report["root"] = json!(self.handle(selected));
        report["match"] = json!({"names": options.names, "mode": "exact"});
        report["columns"] = json!(columns);
        report["rows"] = json!(rows);
        report["selections"] = json!(selections);
        report["page"] = page;
        report["omitted"] = json!({"matching_locals": hidden_locals.iter().sum::<usize>()});
        if options.source {
            report["source_budget"] = json!({"limit_bytes": options.bytes,
                "returned_bytes": returned_bytes, "scope": "returned page"});
        }
        Ok(report)
    }
}
