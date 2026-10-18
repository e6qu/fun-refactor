use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind, Read};
use std::path::{Component, Path, PathBuf};

pub struct Fingerprint {
    pub digest: String,
    pub watched: Vec<PathBuf>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message.into())
}

fn source_error(path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("Cannot read native source {}: {error}.", path.display()),
    )
}

fn normalized(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && value
            .split('/')
            .all(|part| !matches!(part, "" | "." | ".."))
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn collect(
    root: &Path,
    relative: &Path,
    ignored: &BTreeSet<String>,
    files: &mut BTreeSet<PathBuf>,
    directories: &mut BTreeSet<PathBuf>,
) -> io::Result<()> {
    let path = root.join(relative);
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|error| source_error(relative, error))?;
    if metadata.file_type().is_symlink() {
        return Err(invalid(format!(
            "native source input is a symlink: {}.",
            relative.display()
        )));
    }
    if metadata.is_file() {
        files.insert(relative.to_owned());
    } else if metadata.is_dir() {
        directories.insert(relative.to_owned());
        for entry in std::fs::read_dir(&path).map_err(|error| source_error(relative, error))? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| invalid("native source names must be UTF-8."))?;
            if entry.file_type()?.is_dir() && ignored.contains(name) {
                continue;
            }
            collect(root, &relative.join(name), ignored, files, directories)?;
        }
    } else {
        return Err(invalid(format!(
            "native source input is not a regular file or directory: {}.",
            relative.display()
        )));
    }
    Ok(())
}

fn field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

pub fn fingerprint(
    root: &Path,
    configuration: &BTreeMap<String, String>,
) -> io::Result<Fingerprint> {
    let policy = std::fs::read_to_string(root.join("native-source-inputs.txt"))
        .map_err(|error| source_error(Path::new("native-source-inputs.txt"), error))?;
    let mut watched = BTreeMap::new();
    let mut ignored = BTreeSet::new();
    for line in policy.lines() {
        if line.is_empty() || line.trim() != line {
            return Err(invalid(
                "native source policy needs one literal path per line.",
            ));
        }
        if let Some(name) = line.strip_prefix('!') {
            let name = name
                .strip_suffix('/')
                .ok_or_else(|| invalid("only directories may be ignored."))?;
            if !normalized(name) || name.contains('/') || !ignored.insert(name.to_owned()) {
                return Err(invalid(
                    "ignored directory names must be unique single components.",
                ));
            }
        } else {
            let directory = line.ends_with('/');
            let name = line.strip_suffix('/').unwrap_or(line);
            if !normalized(name) || watched.insert(PathBuf::from(name), directory).is_some() {
                return Err(invalid(
                    "native source roots must be unique normalized paths.",
                ));
            }
        }
    }
    if watched.get(Path::new("native-source-inputs.txt")) != Some(&false) {
        return Err(invalid("native source policy must include itself."));
    }
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    for (path, directory) in &watched {
        let metadata = std::fs::symlink_metadata(root.join(path))
            .map_err(|error| source_error(path, error))?;
        if metadata.file_type().is_symlink() || metadata.is_dir() != *directory {
            return Err(invalid(format!(
                "native source root has the wrong kind: {}.",
                path.display()
            )));
        }
        collect(root, path, &ignored, &mut files, &mut directories)?;
    }
    let mut hasher = Sha256::new();
    for path in &files {
        let name = path
            .to_str()
            .ok_or_else(|| invalid("native source paths must be UTF-8."))?
            .replace('\\', "/");
        field(&mut hasher, b"file");
        field(&mut hasher, name.as_bytes());
        let mut file =
            std::fs::File::open(root.join(path)).map_err(|error| source_error(path, error))?;
        let expected = file.metadata()?.len();
        hasher.update(expected.to_le_bytes());
        let mut read = 0_u64;
        let mut buffer = [0_u8; 65_536];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            read += count as u64;
        }
        if read != expected {
            return Err(invalid("native source input changed while hashing."));
        }
    }
    for (name, value) in configuration {
        field(&mut hasher, b"configuration");
        field(&mut hasher, name.as_bytes());
        field(&mut hasher, value.as_bytes());
    }
    let mut watched: BTreeSet<_> = watched.into_keys().collect();
    watched.extend(files);
    watched.extend(directories);
    Ok(Fingerprint {
        digest: hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        watched: watched.into_iter().collect(),
    })
}
