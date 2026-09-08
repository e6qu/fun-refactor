use super::*;

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
}
