{
    let mut path = root.to_path_buf();
    for part in MAP.split('/') {
        path.push(part);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "check scope map cannot traverse symlinks"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), hash("absent-check-scope-map")?, None))
            }
            Err(error) => return Err(error.into()),
        }
    }
    ensure!(path.is_file(), "check scope map must be a regular file");
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65537)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "check scope map exceeds 64 KiB");
    let map: CheckMap = serde_json::from_slice(&bytes)?;
    ensure!(
        map.schema == 1 && map.checks.len() <= 32,
        "invalid check scope map schema or count"
    );
    let mut names = BTreeSet::new();
    for check in &map.checks {
        ensure!(
            names.insert(check.name.clone()) && !check.paths.is_empty() && check.paths.len() <= 64,
            "check scope associations need distinct names and 1..64 paths."
        );
        let mut paths = BTreeSet::new();
        ensure!(
            check
                .paths
                .iter()
                .all(|p| relative(p) && p.len() <= 512 && paths.insert(p)),
            "check scope paths must be distinct normalized relative paths."
        );
    }
    let selected = crate::checks::select(root, &names.into_iter().collect::<Vec<_>>())?;
    Ok((
        map.checks,
        hash(bytes)?,
        selected.map(|s| s.configuration_basis),
    ))
}