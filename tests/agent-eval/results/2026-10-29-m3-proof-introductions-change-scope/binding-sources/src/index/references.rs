//! Derived reference lookups live only until the next mutable borrow.

use crate::model::{Reference, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::OnceLock;

/// An ordered reference vector with lazy target and name lookups.
///
/// Vector operations work through `Deref` and `DerefMut`. Any mutable access
/// discards both lookups before exposing the vector, including element mutation.
/// Convert a replacement vector with `.into()`; use `into_vec()` to take it out.
#[derive(Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct References {
    values: Vec<Reference>,
    #[serde(skip)]
    targets: OnceLock<HashMap<SymbolId, Vec<usize>>>,
    #[serde(skip)]
    names: OnceLock<HashMap<String, Vec<usize>>>,
}

impl References {
    pub fn into_vec(self) -> Vec<Reference> {
        self.values
    }

    fn targets(&self) -> &HashMap<SymbolId, Vec<usize>> {
        self.targets.get_or_init(|| {
            let mut targets = HashMap::<_, Vec<_>>::new();
            for (position, reference) in self.values.iter().enumerate() {
                if let Some(target) = reference.target {
                    targets.entry(target).or_default().push(position);
                }
            }
            targets
        })
    }

    pub(super) fn to_group(&self, group: &[SymbolId]) -> Vec<&Reference> {
        if group.is_empty() {
            return Vec::new();
        }
        let targets = self.targets();
        if let [target] = group {
            return targets
                .get(target)
                .into_iter()
                .flatten()
                .map(|&i| &self.values[i])
                .collect();
        }
        let mut positions = group
            .iter()
            .filter_map(|id| targets.get(id))
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        // Group order is declaration order; callers require reference order.
        positions.sort_unstable();
        positions.dedup();
        positions.into_iter().map(|i| &self.values[i]).collect()
    }

    pub(super) fn count_group(&self, mut group: Vec<SymbolId>) -> usize {
        if group.is_empty() {
            return 0;
        }
        group.sort_unstable();
        group.dedup();
        let targets = self.targets();
        group
            .iter()
            .filter_map(|id| targets.get(id))
            .map(Vec::len)
            .sum()
    }

    pub(super) fn has_group(&self, group: &[SymbolId]) -> bool {
        !group.is_empty() && group.iter().any(|id| self.targets().contains_key(id))
    }

    pub(super) fn matching(&self, name: &str, excluding: SymbolId) -> Vec<&Reference> {
        let names = self.names.get_or_init(|| {
            let mut names = HashMap::<_, Vec<_>>::new();
            for (position, reference) in self.values.iter().enumerate() {
                names
                    .entry(reference.name.clone())
                    .or_default()
                    .push(position);
            }
            names
        });
        names
            .get(name)
            .into_iter()
            .flatten()
            .map(|&i| &self.values[i])
            .filter(|r| r.target != Some(excluding))
            .collect()
    }
}

impl Deref for References {
    type Target = Vec<Reference>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl DerefMut for References {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.targets.take();
        self.names.take();
        &mut self.values
    }
}

impl Clone for References {
    fn clone(&self) -> Self {
        self.values.clone().into()
    }
}

impl std::fmt::Debug for References {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.values.fmt(f)
    }
}

impl From<Vec<Reference>> for References {
    fn from(values: Vec<Reference>) -> Self {
        Self {
            values,
            ..Self::default()
        }
    }
}

impl FromIterator<Reference> for References {
    fn from_iter<T: IntoIterator<Item = Reference>>(iter: T) -> Self {
        Vec::from_iter(iter).into()
    }
}

impl IntoIterator for References {
    type Item = Reference;
    type IntoIter = std::vec::IntoIter<Reference>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

impl<'a> IntoIterator for &'a References {
    type Item = &'a Reference;
    type IntoIter = std::slice::Iter<'a, Reference>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}

impl<'a> IntoIterator for &'a mut References {
    type Item = &'a mut Reference;
    type IntoIter = std::slice::IterMut<'a, Reference>;

    fn into_iter(self) -> Self::IntoIter {
        self.deref_mut().iter_mut()
    }
}
