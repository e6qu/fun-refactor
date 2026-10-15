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

#[derive(PartialEq, Eq)]
pub(super) enum Check {
    Complete,
    UnavailableCycleMember,
    Unknown,
}

#[derive(Default)]
struct State {
    active: BTreeSet<PathBuf>,
    finished: BTreeSet<PathBuf>,
    names: BTreeMap<PathBuf, BTreeSet<String>>,
}

impl Order {
    pub fn check(&self, roots: &[PathBuf]) -> Check {
        let mut state = State::default();
        for file in roots {
            let result = self.execute(file, &mut state);
            if result != Check::Complete {
                return result;
            }
        }
        Check::Complete
    }

    fn execute(&self, file: &Path, state: &mut State) -> Check {
        if state.finished.contains(file) || state.active.contains(file) {
            return Check::Complete;
        }
        let Some(steps) = self.steps.get(file) else {
            return Check::Unknown;
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
                    for path in dependencies {
                        let result = self.execute(path, state);
                        if result != Check::Complete {
                            return result;
                        }
                    }
                    let Some(target) = target else {
                        return Check::Unknown;
                    };
                    if let Some(member) = member {
                        if !state
                            .names
                            .get(target)
                            .is_some_and(|names| names.contains(member))
                        {
                            return if state.active.contains(target) {
                                Check::UnavailableCycleMember
                            } else {
                                Check::Unknown
                            };
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
        Check::Complete
    }
}
