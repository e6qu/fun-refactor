{
        validate(query)?;
        query
            .targets
            .iter()
            .map(|target| {
                let path = self.root.join(&target.path);
                let found: Vec<_> = self
                    .index
                    .symbols
                    .iter()
                    .filter(|s| {
                        s.file == path && s.name == target.name && s.kind.as_str() == target.kind
                    })
                    .collect();
                ensure!(
                    found.len() == 1,
                    "change scope requires one exact declaration per selector."
                );
                Ok(found[0].id)
            })
            .collect()
    }