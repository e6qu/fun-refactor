use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(super) enum Step {
    Bind(Vec<String>),
    Imports {
        items: Vec<Import>,
        from_import: bool,
    },
}

pub(super) struct Import {
    pub dependencies: Vec<PathBuf>,
    pub target: Option<PathBuf>,
    pub member: Option<String>,
    pub alias: Option<String>,
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
                Step::Imports { items, from_import } => {
                    if *from_import {
                        for item in items {
                            let result = item.load(self, state);
                            if result != Check::Complete {
                                return result;
                            }
                        }
                    }
                    for item in items {
                        if !*from_import {
                            let result = item.load(self, state);
                            if result != Check::Complete {
                                return result;
                            }
                        }
                        let result = item.bind(file, state);
                        if result != Check::Complete {
                            return result;
                        }
                    }
                }
            }
        }
        state.active.remove(file);
        state.finished.insert(file.to_owned());
        Check::Complete
    }
}

impl Import {
    fn load(&self, order: &Order, state: &mut State) -> Check {
        for path in &self.dependencies {
            let result = order.execute(path, state);
            if result != Check::Complete {
                return result;
            }
        }
        if self.target.is_none() {
            return Check::Unknown;
        }
        Check::Complete
    }

    fn bind(&self, file: &Path, state: &mut State) -> Check {
        let Some(target) = &self.target else {
            return Check::Unknown;
        };
        if let Some(member) = &self.member {
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
        if let Some(alias) = &self.alias {
            state
                .names
                .entry(file.to_owned())
                .or_default()
                .insert(alias.clone());
        }
        Check::Complete
    }
}
