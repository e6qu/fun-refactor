{
    self.scope_seeds(query)?;
    hash((
        &self.revision, query, map, checks,
        include_str!("change_scope.rs"), include_str!("../index.rs"),
        include_str!("../index/references.rs"), include_str!("../model.rs"),
        include_str!("../analysis/entrypoints.rs"), include_str!("occurrence.rs"),
        include_str!("relationships.rs"), format!("{:?}", Catalog::builtin()?.rules),
    ))
}
