use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(super) enum Step {
    Bind(Vec<String>),
    Import {
        dependencies: Vec<PathBuf>,
        target: Option<PathBuf>,
        member: Option<String>,
        alias: Option<String>,
    },
}

#[derive(Default)]
pub(super) struct Order {
    pub steps: BTreeMap<PathBuf, Vec<Step>>,
}

#[derive(Default)]
struct State {
    active: BTreeSet<PathBuf>,
    finished: BTreeSet<PathBuf>,
    names: BTreeMap<PathBuf, BTreeSet<String>>,
}

impl Order {
    pub fn valid(&self, roots: &[PathBuf]) -> bool {
        let mut state = State::default();
        roots.iter().all(|file| self.execute(file, &mut state))
    }

    fn execute(&self, file: &Path, state: &mut State) -> bool {
        if state.finished.contains(file) || state.active.contains(file) {
            return true;
        }
        let Some(steps) = self.steps.get(file) else {
            return false;
        };
        state.active.insert(file.to_owned());
        for step in steps {
            match step {
                Step::Bind(names) => {
                    state
                        .names
                        .entry(file.to_owned())
                        .or_default()
                        .extend(names.iter().cloned());
                }
                Step::Import {
                    dependencies,
                    target,
                    member,
                    alias,
                } => {
                    if !dependencies.iter().all(|path| self.execute(path, state)) {
                        return false;
                    }
                    if let Some(member) = member {
                        if !target
                            .as_ref()
                            .and_then(|path| state.names.get(path))
                            .is_some_and(|names| names.contains(member))
                        {
                            return false;
                        }
                    }
                    if let Some(alias) = alias {
                        state
                            .names
                            .entry(file.to_owned())
                            .or_default()
                            .insert(alias.clone());
                    }
                }
            }
        }
        state.active.remove(file);
        state.finished.insert(file.to_owned());
        true
    }
}
