{
    ensure!(
        !query.targets.is_empty()
            && query.targets.len() <= 16
            && query.depth <= 16
            && (1..=256).contains(&query.nodes)
            && (1..=4096).contains(&query.references)
            && (1024..=1048576).contains(&query.bytes),
        "invalid change scope budgets"
    );
    let mut distinct = BTreeSet::new();
    for target in &query.targets {
        ensure!(
            relative(&target.path)
                && target.path.len() <= 512
                && !target.name.is_empty()
                && target.name.len() <= 256
                && distinct.insert((&target.path, &target.name, &target.kind)),
            "change scope requires distinct bounded declaration selectors."
        );
    }
    Ok(())
}