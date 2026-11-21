use super::*;

const ITEMS: usize = 64;

#[derive(Clone)]
pub(super) struct References {
    pub ids: BTreeSet<String>,
    pub scalar: bool,
}

impl Default for References {
    fn default() -> Self {
        Self {
            ids: BTreeSet::new(),
            scalar: true,
        }
    }
}

impl References {
    pub fn join(&mut self, other: &Self) -> bool {
        let before = self.ids.len();
        self.ids.extend(other.ids.iter().cloned());
        let changed = before != self.ids.len() || !self.scalar && other.scalar;
        self.scalar |= other.scalar;
        changed
    }
}

#[derive(Clone, Default)]
pub(super) struct Element {
    pub flow: Flow,
    pub references: References,
    pub facts: Facts,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct Facts {
    pub plain_string: bool,
    pub noniterable: bool,
}

impl Element {
    fn join(&mut self, other: &Self) -> bool {
        let before = self.flow.len();
        merge(&mut self.flow, other.flow.clone());
        let facts = self.facts;
        self.facts.plain_string &= other.facts.plain_string;
        self.facts.noniterable &= other.facts.noniterable;
        self.references.join(&other.references) | (before != self.flow.len()) | (facts != self.facts)
    }
}

#[derive(Clone, Default)]
pub(super) struct Lists {
    pub bindings: BTreeMap<String, BTreeSet<String>>,
    uncertain: BTreeSet<String>,
    objects: BTreeMap<String, List>,
    expressions: BTreeMap<(PathBuf, usize), References>,
    facts: BTreeMap<String, Facts>,
    expression_facts: BTreeMap<(PathBuf, usize), Facts>,
}

#[derive(Clone)]
pub(super) struct List {
    pub items: Vec<Element>,
    pub minimum: usize,
    pub tuple: bool,
}

struct IndexedList {
    slots: BTreeMap<String, BTreeSet<usize>>,
    receiver: Flow,
}

fn through_alias(mut current: Flow, prior: &Flow) -> Flow {
    // Keep alias routes only for origins still in the object after mutation.
    for (origin, trace) in &mut current {
        if let Some(previous) = prior.get(origin) {
            for point in &previous.occurrences {
                if !trace
                    .occurrences
                    .iter()
                    .any(|existing| existing.id == point.id)
                {
                    trace.occurrences.push(point.clone());
                }
            }
        }
    }
    current
}

impl Lists {
    pub fn forget(&mut self, name: &str) {
        self.bindings.remove(name);
        self.uncertain.remove(name);
        self.facts.remove(name);
    }

    pub fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        let before = self.facts.len();
        self.facts.retain(|name, facts| other.facts.get(name) == Some(facts));
        changed |= self.facts.len() != before;
        let before = self.uncertain.len();
        for name in self.bindings.keys().chain(other.bindings.keys()) {
            if self.bindings.contains_key(name) != other.bindings.contains_key(name) {
                self.uncertain.insert(name.clone());
            }
        }
        self.uncertain.extend(other.uncertain.iter().cloned());
        changed |= before != self.uncertain.len();
        for (name, ids) in &other.bindings {
            let target = self.bindings.entry(name.clone()).or_default();
            let before = target.len();
            target.extend(ids.iter().cloned());
            changed |= before != target.len();
        }
        for (id, list) in &other.objects {
            match self.objects.get_mut(id) {
                None => {
                    self.objects.insert(id.clone(), list.clone());
                    changed = true;
                }
                Some(target) => {
                    let minimum = target.minimum.min(list.minimum);
                    changed |= minimum != target.minimum;
                    target.minimum = minimum;
                    if list.items.len() > target.items.len() {
                        target.items.resize_with(list.items.len(), Element::default);
                        changed = true;
                    }
                    for (left, right) in target.items.iter_mut().zip(&list.items) {
                        changed |= left.join(right);
                    }
                }
            }
        }
        changed
    }

    pub fn references(&self, node: Node<'_>, file: &Path, source: &str) -> References {
        let Some(node) = calls::unparenthesized(node) else {
            return References::default();
        };
        if node.kind() == "identifier" {
            let name = &source[node.byte_range()];
            return self
                .bindings
                .get(name)
                .map_or_else(References::default, |ids| References {
                    ids: ids.clone(),
                    scalar: self.uncertain.contains(name),
                });
        }
        self.expressions
            .get(&(file.to_owned(), node.start_byte()))
            .cloned()
            .unwrap_or_default()
    }

    pub fn contents(&self, ids: &BTreeSet<String>) -> Flow {
        let mut result = Flow::new();
        let mut visited = BTreeSet::new();
        let mut pending: Vec<_> = ids.iter().collect();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(list) = self.objects.get(id) {
                for item in &list.items {
                    merge(&mut result, item.flow.clone());
                    pending.extend(item.references.ids.iter());
                }
            }
        }
        result
    }
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk())
        .filter(|n| n.kind() != "comment")
        .collect()
}

fn integer_value(node: Node<'_>, source: &str) -> Option<i128> {
    let node = calls::unparenthesized(node)?;
    if node.kind() == "unary_operator" {
        let operator = node.child_by_field_name("operator")?;
        let value = integer_value(node.child_by_field_name("argument")?, source)?;
        return match &source[operator.byte_range()] {
            "+" => Some(value),
            "-" => value.checked_neg(),
            _ => None,
        };
    }
    if node.kind() != "integer" {
        return None;
    }
    let text = source[node.byte_range()]
        .replace('_', "")
        .to_ascii_lowercase();
    let (digits, radix) = if let Some(digits) = text.strip_prefix("0x") {
        (digits, 16)
    } else if let Some(digits) = text.strip_prefix("0o") {
        (digits, 8)
    } else if let Some(digits) = text.strip_prefix("0b") {
        (digits, 2)
    } else {
        (text.as_str(), 10)
    };
    i128::from_str_radix(digits, radix).ok()
}

fn integer_index(node: Node<'_>, source: &str) -> Option<i64> {
    integer_value(node, source)?.try_into().ok()
}

fn list_slot(index: i64, length: usize) -> Option<usize> {
    let slot = if index < 0 {
        (length as i64).checked_add(index)?
    } else {
        index
    };
    usize::try_from(slot).ok().filter(|slot| *slot < length)
}

struct Slice {
    start: Option<i64>,
    stop: Option<i64>,
    step: i64,
}

impl Slice {
    fn positions(&self, length: usize) -> Vec<usize> {
        let length = length as i128;
        let step = i128::from(self.step);
        let lower = if step > 0 { 0 } else { -1 };
        let upper = if step > 0 { length } else { length - 1 };
        let bound = |value: i64| {
            let value = i128::from(value);
            (if value < 0 { length + value } else { value }).clamp(lower, upper)
        };
        let mut cursor = self
            .start
            .map_or(if step > 0 { 0 } else { length - 1 }, bound);
        let stop = self.stop.map_or(if step > 0 { length } else { -1 }, bound);
        let mut positions = Vec::new();
        while if step > 0 {
            cursor < stop
        } else {
            cursor > stop
        } {
            positions.push(cursor as usize);
            cursor += step;
        }
        positions
    }

    fn insertion(&self, length: usize) -> usize {
        let length = length as i128;
        self.start.map_or(0, |start| {
            let start = i128::from(start);
            (if start < 0 { length + start } else { start }).clamp(0, length) as usize
        })
    }
}

fn slice_node(node: Node<'_>) -> Option<Node<'_>> {
    let mut indices = node
        .children_by_field_name("subscript", &mut node.walk())
        .collect::<Vec<_>>();
    if indices.len() != 1
        || node
            .children(&mut node.walk())
            .any(|child| child.kind() == ",")
    {
        return None;
    }
    indices.pop().filter(|index| index.kind() == "slice")
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn list_references(&self, node: Node<'t>, env: &Environment) -> References {
        env.lists.borrow().references(node, self.file, self.source)
    }

    pub(super) fn record_list_references(
        &self,
        node: Node<'t>,
        env: &Environment,
        refs: References,
    ) {
        let key = (self.file.to_owned(), node.start_byte());
        env.lists.borrow_mut().expression_facts.remove(&key);
        if refs.ids.is_empty() {
            env.lists.borrow_mut().expressions.remove(&key);
        } else {
            env.lists.borrow_mut().expressions.insert(key, refs);
        }
    }

    pub(super) fn list_element(&self, node: Node<'t>, flow: Flow, env: &Environment) -> Element {
        let references = self.list_references(node, env);
        let bare = calls::unparenthesized(node).unwrap_or(node);
        let heap = env.lists.borrow();
        let facts = heap.expression_facts.get(&(self.file.to_owned(), node.start_byte()))
            .copied().or_else(|| (bare.kind() == "identifier")
                .then(|| heap.facts.get(self.text(bare)).copied()).flatten())
            .unwrap_or(Facts {
                plain_string: self.plain_string(node, env),
                noniterable: matches!(bare.kind(), "integer" | "float" | "true" | "false" | "none"),
            });
        Element {
            flow: if references.scalar { flow } else { Flow::new() },
            references,
            facts,
        }
    }

    pub(super) fn element_flow(&self, value: &Element, env: &Environment) -> Flow {
        let mut flow = value.flow.clone();
        merge(
            &mut flow,
            env.lists.borrow().contents(&value.references.ids),
        );
        flow
    }

    pub(super) fn list_literal(&mut self, node: Node<'t>, env: &Environment) -> Flow {
        let mut values = Vec::new();
        for item in children(node) {
            if item.kind() == "list_splat" {
                let Some(argument) = item.named_child(0) else {
                    self.cutoff("sequence-expansion-unchecked");
                    return Flow::new();
                };
                let flow = self.expression(argument, env);
                if !self.normal { return Flow::new(); }
                let value = self.list_element(argument, flow, env);
                let Some(items) = self.sequence_items(item, &value, env) else {
                    return Flow::new();
                };
                values.extend(items);
            } else {
                let flow = self.expression(item, env);
                if !self.normal { return Flow::new(); }
                values.push(self.list_element(item, flow, env));
            }
            if values.len() > ITEMS {
                self.cutoff("list-item-budget");
                return Flow::new();
            }
        }
        self.allocate_sequence(node, values, env, matches!(node.kind(), "tuple" | "expression_list"))
    }

    pub(super) fn allocate_list(&mut self, node: Node<'t>, values: Vec<Element>, env: &Environment) -> Flow {
        self.allocate_sequence(node, values, env, false)
    }

    fn allocate_sequence(&mut self, node: Node<'t>, values: Vec<Element>, env: &Environment, tuple: bool) -> Flow {
        if values.len() > ITEMS {
            self.cutoff("list-item-budget");
            return Flow::new();
        }
        let id = format!(
            "{}:{}:{}:{}:{}",
            self.list_sites.join("/"),
            self.file.display(),
            node.start_byte(),
            node.end_byte(),
            node.kind()
        );
        if env.lists.borrow().objects.contains_key(&id) {
            self.cutoff("repeated-list-allocation");
            return Flow::new();
        }
        if env.lists.borrow().objects.len() >= ITEMS {
            self.cutoff("list-object-budget");
            return Flow::new();
        }
        env.lists.borrow_mut().objects.insert(
            id.clone(),
            List {
                minimum: values.len(),
                items: values,
                tuple,
            },
        );
        let ids = BTreeSet::from([id]);
        let flow = env.lists.borrow().contents(&ids);
        self.record_list_references(node, env, References { ids, scalar: false });
        self.extend(flow, node, "list-allocation")
    }

    pub(super) fn exact_list(&mut self, refs: &References, env: &Environment) -> Option<List> {
        if refs.scalar || refs.ids.len() != 1 {
            self.cutoff("list-sequence-target-unchecked");
            return None;
        }
        let list = env.lists.borrow().objects.get(refs.ids.first()?).cloned();
        if list
            .as_ref()
            .is_none_or(|list| list.minimum != list.items.len())
        {
            self.cutoff("list-sequence-length-unchecked");
            return None;
        }
        list
    }

    fn slice_spec(&mut self, node: Node<'t>, env: &Environment) -> Option<Slice> {
        let mut fields = [None; 3];
        let mut index = 0;
        let mut valid = true;
        for child in node.children(&mut node.walk()) {
            if child.kind() == ":" {
                index += 1;
            } else if child.is_named() && !child.is_extra() {
                self.expression(child, env);
                if !self.normal {
                    return None;
                }
                if child.kind() != "none" {
                    let value = integer_index(child, self.source);
                    valid &= value.is_some();
                    if let Some(field) = fields.get_mut(index) {
                        *field = value;
                    } else {
                        valid = false;
                    }
                }
            }
        }
        if !valid || !(1..=2).contains(&index) {
            self.cutoff("list-slice-bounds-unchecked");
            return None;
        }
        let step = fields[2].unwrap_or(1);
        Some(Slice {
            start: fields[0],
            stop: fields[1],
            step,
        })
    }

    fn slice_target(
        &mut self,
        node: Node<'t>,
        slice: Node<'t>,
        env: &Environment,
    ) -> Option<(String, List, Slice, Flow)> {
        let receiver = node.child_by_field_name("value")?;
        let flow = self.expression(receiver, env);
        if !self.normal {
            return None;
        }
        let refs = self.list_references(receiver, env);
        let slice = self.slice_spec(slice, env)?;
        let list = self.exact_list(&refs, env)?;
        if slice.step == 0 {
            self.list_error(node, env, "ValueError", true);
            return None;
        }
        Some((refs.ids.first()?.clone(), list, slice, flow))
    }

    fn list_slice(&mut self, node: Node<'t>, slice: Node<'t>, env: &Environment) -> Flow {
        let Some((_, list, slice, receiver)) = self.slice_target(node, slice, env) else {
            return Flow::new();
        };
        let items = slice
            .positions(list.items.len())
            .into_iter()
            .map(|index| list.items[index].clone())
            .collect();
        let flow = self.allocate_sequence(node, items, env, list.tuple);
        through_alias(flow, &receiver)
    }

    fn write_slice(
        &mut self,
        node: Node<'t>,
        slice: Node<'t>,
        replacement: &References,
        env: &Environment,
    ) {
        let Some((id, mut list, slice, _)) = self.slice_target(node, slice, env) else {
            return;
        };
        if list.tuple { self.cutoff("tuple-mutation-unchecked"); return; }
        let Some(replacement) = self.exact_list(replacement, env) else {
            return;
        };
        let positions = slice.positions(list.items.len());
        let items = replacement.items;
        if slice.step != 1 && positions.len() != items.len() {
            self.list_error(node, env, "ValueError", true);
            return;
        }
        if list.items.len() - positions.len() + items.len() > ITEMS {
            self.cutoff("list-item-budget");
            return;
        }
        if slice.step == 1 {
            let start = slice.insertion(list.items.len());
            list.items.splice(start..start + positions.len(), items);
        } else {
            for (position, item) in positions.into_iter().zip(items) {
                list.items[position] = item;
            }
        }
        list.minimum = list.items.len();
        env.lists.borrow_mut().objects.insert(id, list);
    }

    pub(super) fn list_delete(&mut self, node: Node<'t>, env: &Environment) {
        for target in children(node) {
            if matches!(target.kind(), "expression_list" | "tuple") {
                self.list_delete(target, env);
            } else if target.kind() == "subscript" {
                if let Some(slice) = slice_node(target) {
                    if let Some((id, mut list, slice, _)) = self.slice_target(target, slice, env) {
                        if list.tuple { self.cutoff("tuple-mutation-unchecked"); return; }
                        let positions: BTreeSet<_> =
                            slice.positions(list.items.len()).into_iter().collect();
                        list.items = list
                            .items
                            .into_iter()
                            .enumerate()
                            .filter_map(|(index, item)| {
                                (!positions.contains(&index)).then_some(item)
                            })
                            .collect();
                        list.minimum = list.items.len();
                        env.lists.borrow_mut().objects.insert(id, list);
                    }
                } else if let Some(indexed) = self.indexed_list(target, env) {
                    if !self.normal {
                        return;
                    }
                    if indexed.slots.len() != 1
                        || indexed.slots.values().any(|slots| slots.len() != 1)
                    {
                        self.cutoff("list-mutation-target-unchecked");
                        continue;
                    }
                    let mut heap = env.lists.borrow_mut();
                    for (id, positions) in indexed.slots {
                        let Some(list) = heap.objects.get_mut(&id) else {
                            continue;
                        };
                        if list.tuple { self.cutoff("tuple-mutation-unchecked"); continue; }
                        if list.minimum != list.items.len() {
                            self.cutoff("list-sequence-length-unchecked");
                            continue;
                        }
                        list.items.remove(*positions.first().unwrap());
                        list.minimum -= 1;
                    }
                } else {
                    self.cutoff("list-delete-target-unchecked");
                }
            } else {
                self.cutoff("list-delete-target-unchecked");
            }
            if !self.normal {
                return;
            }
        }
    }

    pub(super) fn list_binary(&mut self, node: Node<'t>, env: &Environment, flow: Flow) -> Flow {
        if !self.normal {
            return flow;
        }
        let Some(left) = node.child_by_field_name("left") else {
            return flow;
        };
        let Some(right) = node.child_by_field_name("right") else {
            return flow;
        };
        let left = self.list_references(left, env);
        let right = self.list_references(right, env);
        if left.ids.is_empty() && right.ids.is_empty() {
            return flow;
        }
        if !node
            .child_by_field_name("operator")
            .is_some_and(|operator| self.text(operator) == "+")
        {
            self.cutoff("list-operator-unchecked");
            return flow;
        }
        let Some(mut left) = self.exact_list(&left, env) else {
            return flow;
        };
        let Some(right) = self.exact_list(&right, env) else {
            return flow;
        };
        if left.tuple != right.tuple {
            self.cutoff("mixed-sequence-concatenation-unchecked");
            return flow;
        }
        left.items.extend(right.items);
        let result = self.allocate_sequence(node, left.items, env, left.tuple);
        through_alias(result, &flow)
    }

    pub(super) fn list_value(&mut self, node: Node<'t>, env: &Environment) -> Option<Flow> {
        let node = calls::unparenthesized(node)?;
        if node.kind() != "identifier" {
            return None;
        }
        let ids = self.list_references(node, env).ids;
        if ids.is_empty() {
            return None;
        }
        if !env
            .defined
            .contains(self.text(calls::unparenthesized(node)?))
        {
            self.cutoff("possibly-unbound-list");
        }
        if env
            .lists
            .borrow()
            .uncertain
            .contains(self.text(calls::unparenthesized(node)?))
        {
            self.cutoff("list-binding-type-unchecked");
        }
        let mut flow = env.lists.borrow().contents(&ids);
        if let Some(prior) = env.values.get(self.text(node)) {
            flow = through_alias(flow, prior);
        }
        Some(self.extend(flow, node, "list-read"))
    }

    pub(super) fn list_assignment(&mut self, assignment: Node<'t>, env: &mut Environment) -> bool {
        let (Some(left), Some(right)) = (
            assignment.child_by_field_name("left"),
            assignment.child_by_field_name("right"),
        ) else {
            return false;
        };
        if assignment.child_by_field_name("type").is_some() {
            return false;
        }
        if left.kind() == "subscript" {
            return self.list_write(left, right, env);
        }
        if left.kind() != "identifier" {
            return false;
        }
        let Some(right) = calls::unparenthesized(right) else {
            return false;
        };
        if right.kind() == "assignment" {
            return false;
        }
        let flow = self.expression(right, env);
        if self.normal {
            let value = self.list_element(right, flow, env);
            self.bind_element(left, &value, env);
        }
        true
    }

    pub(super) fn bind_element(&mut self, target: Node<'t>, value: &Element, env: &mut Environment) {
        if !self.tick(target, "definition") { return; }
        let name = self.text(target).to_owned();
        let flow = self.extend(self.element_flow(value, env), target, "definition");
        let mut heap = env.lists.borrow_mut();
        if value.references.ids.is_empty() {
            heap.bindings.remove(&name);
        } else {
            heap.bindings.insert(name.clone(), value.references.ids.clone());
        }
        if value.references.scalar && !value.references.ids.is_empty() {
            heap.uncertain.insert(name.clone());
        } else { heap.uncertain.remove(&name); }
        heap.facts.insert(name.clone(), value.facts);
        env.values.insert(name.clone(), flow);
        env.defined.insert(name.clone());
        if value.facts.plain_string { env.plain_strings.insert(name); }
        else { env.plain_strings.remove(&name); }
    }

    fn indexed_list(&mut self, node: Node<'t>, env: &Environment) -> Option<IndexedList> {
        let value = node.child_by_field_name("value")?;
        let receiver = self.expression(value, env);
        if !self.normal {
            return Some(IndexedList {
                slots: BTreeMap::new(),
                receiver,
            });
        }
        let references = self.list_references(value, env);
        let ids = references.ids;
        if ids.is_empty() {
            return None;
        }
        if references.scalar {
            self.cutoff("list-binding-type-unchecked");
        }
        if node
            .children_by_field_name("subscript", &mut node.walk())
            .count()
            != 1
            || node
                .children(&mut node.walk())
                .any(|child| child.kind() == ",")
        {
            self.cutoff("list-index-unchecked");
            return Some(IndexedList {
                slots: BTreeMap::new(),
                receiver,
            });
        }
        let index = node.child_by_field_name("subscript")?;
        self.expression(index, env);
        if !self.normal {
            return Some(IndexedList {
                slots: BTreeMap::new(),
                receiver,
            });
        }
        let Some(index_value) = integer_index(index, self.source) else {
            self.cutoff("list-index-unchecked");
            return Some(IndexedList {
                slots: BTreeMap::new(),
                receiver,
            });
        };
        let mut selected = BTreeMap::new();
        let mut invalid = false;
        let mut unknown = references.scalar;
        for id in ids {
            let heap = env.lists.borrow();
            let Some(list) = heap.objects.get(&id) else {
                self.cutoff("list-index-may-raise");
                unknown = true;
                continue;
            };
            let mut slots = BTreeSet::new();
            for length in list.minimum..=list.items.len() {
                if let Some(slot) = list_slot(index_value, length) {
                    slots.insert(slot);
                } else {
                    invalid = true;
                }
            }
            if !slots.is_empty() {
                selected.insert(id, slots);
            }
        }
        if invalid {
            self.list_error(node, env, "IndexError", selected.is_empty() && !unknown);
        }
        Some(IndexedList {
            slots: selected,
            receiver,
        })
    }

    pub(super) fn list_item(&mut self, node: Node<'t>, env: &Environment) -> Option<Flow> {
        if let Some(slice) = slice_node(node) {
            return Some(self.list_slice(node, slice, env));
        }
        let selected = self.indexed_list(node, env)?;
        if !self.normal {
            return Some(Flow::new());
        }
        let mut value = None::<Element>;
        {
            let heap = env.lists.borrow();
            for (id, slots) in selected.slots {
                if let Some(list) = heap.objects.get(&id) {
                    for slot in slots {
                        if let Some(item) = list.items.get(slot) {
                            match &mut value {
                                Some(value) => {
                                    value.join(item);
                                }
                                None => value = Some(item.clone()),
                            }
                        }
                    }
                }
            }
        }
        let value = value.unwrap_or_default();
        self.record_list_references(node, env, value.references.clone());
        env.lists.borrow_mut().expression_facts.insert((self.file.to_owned(), node.start_byte()), value.facts);
        let flow = through_alias(self.element_flow(&value, env), &selected.receiver);
        Some(self.extend(flow, node, "list-item"))
    }

    fn list_write(&mut self, left: Node<'t>, right: Node<'t>, env: &Environment) -> bool {
        if left.child_by_field_name("value").is_none() {
            return false;
        }
        let flow = self.expression(right, env);
        if !self.normal {
            return true;
        }
        let mut value = self.list_element(right, flow, env);
        if let Some(slice) = slice_node(left) {
            self.write_slice(left, slice, &value.references, env);
            return true;
        }
        value.flow = self.extend(value.flow, left, "list-write");
        let Some(indexed) = self.indexed_list(left, env) else {
            self.cutoff("list-write-target-unchecked");
            self.effect_limit(
                left,
                "shared-write",
                "The assignment target is not an established local list.",
            );
            return true;
        };
        let selected = indexed.slots;
        if !self.normal {
            return true;
        }
        let strong = selected.len() == 1 && selected.values().all(|slots| slots.len() == 1);
        let mut heap = env.lists.borrow_mut();
        for (id, slots) in selected {
            if let Some(list) = heap.objects.get_mut(&id) {
                if list.tuple { self.cutoff("tuple-mutation-unchecked"); continue; }
                for slot in slots {
                    if let Some(item) = list.items.get_mut(slot) {
                        if strong {
                            *item = value.clone();
                        } else {
                            item.join(&value);
                        }
                    }
                }
            }
        }
        true
    }

    pub(super) fn list_method(&mut self, node: Node<'t>, env: &Environment) -> Option<Flow> {
        let function = node.child_by_field_name("function")?;
        if function.kind() != "attribute" {
            return None;
        }
        let object = function.child_by_field_name("object")?;
        let object_kind = calls::unparenthesized(object)?.kind();
        if matches!(object_kind, "identifier" | "attribute")
            && self.list_references(object, env).ids.is_empty()
        {
            return None;
        }
        let receiver = self.expression(object, env);
        if !self.normal {
            return Some(Flow::new());
        }
        let references = self.list_references(object, env);
        let ids = references.ids;
        if ids.is_empty() {
            return None;
        }
        if references.scalar {
            self.cutoff("list-binding-type-unchecked");
        }
        if ids.iter().any(|id| env.lists.borrow().objects.get(id).is_some_and(|value| value.tuple)) {
            self.cutoff("tuple-method-unchecked");
            return Some(Flow::new());
        }
        let method = self
            .text(function.child_by_field_name("attribute")?)
            .to_owned();
        let arguments = children(node.child_by_field_name("arguments")?);
        let (minimum, maximum) = match method.as_str() {
            "append" | "extend" => (1, 1),
            "clear" | "reverse" | "copy" => (0, 0),
            "insert" => (2, 2),
            "pop" => (0, 1),
            _ => {
                self.cutoff("list-method-unchecked");
                return Some(Flow::new());
            }
        };
        if !(minimum..=maximum).contains(&arguments.len())
            || arguments.iter().any(|n| {
                matches!(
                    n.kind(),
                    "keyword_argument" | "list_splat" | "dictionary_splat"
                )
            })
        {
            self.cutoff("list-method-arguments");
            return Some(Flow::new());
        }
        let mut values = Vec::new();
        for argument in &arguments {
            let flow = self.expression(*argument, env);
            values.push(self.list_element(*argument, flow, env));
            if !self.normal {
                return Some(Flow::new());
            }
        }
        if method == "copy" {
            let refs = References {
                ids,
                scalar: references.scalar,
            };
            let Some(list) = self.exact_list(&refs, env) else {
                return Some(Flow::new());
            };
            let flow = self.allocate_list(node, list.items, env);
            return Some(through_alias(flow, &receiver));
        }
        if matches!(method.as_str(), "pop" | "insert" | "reverse") {
            let index = if method == "reverse" || arguments.is_empty() {
                -1
            } else if let Some(index) = integer_index(arguments[0], self.source) {
                index
            } else {
                self.cutoff("list-index-unchecked");
                return Some(Flow::new());
            };
            let strong = ids.len() == 1;
            let mut result = None::<Element>;
            for id in ids {
                let mut heap = env.lists.borrow_mut();
                let list = heap.objects.get_mut(&id)?;
                if !strong || list.minimum != list.items.len() {
                    self.cutoff("list-mutation-target-unchecked");
                    if method == "pop" {
                        for item in &list.items {
                            match &mut result {
                                Some(value) => {
                                    value.join(item);
                                }
                                None => result = Some(item.clone()),
                            }
                        }
                    }
                    continue;
                }
                match method.as_str() {
                    "pop" => {
                        if let Some(slot) = list_slot(index, list.items.len()) {
                            result = Some(list.items.remove(slot));
                            list.minimum -= 1;
                        } else {
                            drop(heap);
                            self.list_error(node, env, "IndexError", !references.scalar);
                            return Some(Flow::new());
                        }
                    }
                    "insert" => {
                        if list.items.len() == ITEMS {
                            self.cutoff("list-item-budget");
                            continue;
                        }
                        let length = list.items.len() as i64;
                        let slot = if index < 0 {
                            length.saturating_add(index)
                        } else {
                            index
                        }
                        .clamp(0, length) as usize;
                        let mut value = values[1].clone();
                        value.flow = self.extend(value.flow, node, "list-write");
                        list.items.insert(slot, value);
                        list.minimum += 1;
                    }
                    "reverse" => list.items.reverse(),
                    _ => unreachable!(),
                }
            }
            let result = result.unwrap_or_default();
            self.record_list_references(node, env, result.references.clone());
            env.lists.borrow_mut().expression_facts.insert((self.file.to_owned(), node.start_byte()), result.facts);
            return Some(self.extend(self.element_flow(&result, env), node, "list-method-result"));
        }
        let addition = if method == "extend" {
            let refs = &values[0].references;
            if refs.ids.len() != 1 || refs.scalar {
                self.cutoff("list-extend-source-unchecked");
                return Some(Flow::new());
            }
            let heap = env.lists.borrow();
            let list = &heap.objects[refs.ids.first().unwrap()];
            if list.minimum != list.items.len() {
                self.cutoff("list-extend-length-unchecked");
            }
            list.items.clone()
        } else {
            values
        };
        let strong = ids.len() == 1;
        for id in ids {
            let mut heap = env.lists.borrow_mut();
            let list = heap.objects.get_mut(&id)?;
            if method == "clear" {
                if strong {
                    list.items.clear();
                }
                list.minimum = 0;
            } else if list.items.len() + addition.len() > ITEMS {
                self.cutoff("list-item-budget");
            } else {
                if !strong || list.minimum != list.items.len() {
                    self.cutoff("list-mutation-target-unchecked");
                }
                list.minimum += addition.len();
                list.items.extend(addition.iter().cloned().map(|mut value| {
                    value.flow = self.extend(value.flow, node, "list-write");
                    value
                }));
            }
        }
        Some(Flow::new())
    }

    pub(super) fn list_call(
        &mut self,
        name: &str,
        function: Node<'t>,
        evaluated: &calls::Arguments,
        call: Node<'t>,
        env: &Environment,
    ) -> Option<Flow> {
        let arguments = &evaluated.values;
        let names = &evaluated.names;
        let refs: Vec<_> = evaluated.elements.iter().map(|value| value.references.clone()).collect();
        if refs.iter().all(|value| value.ids.is_empty()) && !self.list_factory(name) {
            return None;
        }
        if self.active.iter().any(|active| active == name) || self.active.len() >= self.depth {
            self.cutoff("list-call-depth-budget");
            return Some(Flow::new());
        }
        let (file, source) = self.contexts[name];
        let Some(parameters) = self.function_parameters(function, file, source) else {
            self.cutoff("list-call-signature-unchecked");
            return Some(Flow::new());
        };
        let Some(slots) = calls::argument_slots(&parameters, source, names) else {
            self.cutoff("list-call-signature-unchecked");
            return Some(Flow::new());
        };
        let Some(arguments) = self.bind_arguments(name, function, arguments.to_vec(), names) else {
            return Some(Flow::new());
        };
        let mut heap = env.lists.borrow().clone();
        heap.bindings.clear();
        heap.uncertain.clear();
        heap.expressions.clear();
        heap.expression_facts.clear();
        heap.facts.clear();
        for (parameter, slot) in parameters.into_iter().zip(slots) {
            if let Some(value) = slot.and_then(|slot| evaluated.elements.get(slot)) {
                heap.facts.insert(source[parameter.node.byte_range()].to_owned(), value.facts);
            }
            if let Some(refs) = slot
                .and_then(|slot| refs.get(slot))
                .filter(|refs| !refs.ids.is_empty())
            {
                let parameter = source[parameter.node.byte_range()].to_owned();
                heap.bindings.insert(parameter.clone(), refs.ids.clone());
                if refs.scalar {
                    heap.uncertain.insert(parameter);
                }
            }
        }
        let saved = (
            self.file,
            self.source,
            std::mem::take(&mut self.returned_lists),
            std::mem::take(&mut self.return_references),
            self.returned_raised_lists.take(),
        );
        self.list_sites
            .push(format!("{}:{}", self.file.display(), call.start_byte()));
        (self.file, self.source) = (file, source);
        let result = self.invoke_with_lists(name, function, arguments, heap);
        self.list_sites.pop();
        let references = self.return_references.clone();
        if let Some(raised) = self.returned_raised_lists.clone() {
            let mut heap = env.lists.borrow().clone();
            heap.objects = raised.objects;
            self.capture_raised_lists(heap.clone());
            if !self.normal {
                env.lists.borrow_mut().objects = heap.objects;
            }
        }
        if self.normal {
            env.lists.borrow_mut().objects = self.returned_lists.objects.clone();
        }
        (
            self.file,
            self.source,
            self.returned_lists,
            self.return_references,
            self.returned_raised_lists,
        ) = saved;
        self.record_list_references(call, env, references);
        Some(self.extend(result, call, "list-helper-result"))
    }

    fn list_factory(&mut self, name: &str) -> bool {
        if let Some(value) = self.list_factories.get(name) {
            return *value;
        }
        let mut pending = vec![name.to_owned()];
        let mut visited = BTreeSet::new();
        let mut found = false;
        while let Some(name) = pending.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            let Some(function) = self.functions.get(&name) else {
                continue;
            };
            let (file, source) = self.contexts[&name];
            let mut nodes: Vec<_> = function.child_by_field_name("body").into_iter().collect();
            while let Some(node) = nodes.pop() {
                if matches!(node.kind(), "list" | "tuple" | "expression_list")
                    && !std::iter::successors(node.parent(), |node| node.parent())
                        .take_while(|node| matches!(node.kind(), "assignment" | "tuple" | "list" | "expression_list" | "parenthesized_expression"))
                        .find(|node| node.kind() == "assignment")
                        .is_some_and(assignments::literal_transfer) {
                    found = true;
                    break;
                }
                if matches!(
                    node.kind(),
                    "function_definition" | "class_definition" | "lambda"
                ) {
                    continue;
                }
                if node.kind() == "call" {
                    if let Some(target) = node.child_by_field_name("function") {
                        let text: String = source[target.byte_range()]
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect();
                        let target = self.modules.map_or_else(
                            || {
                                self.function_aliases
                                    .get(&text)
                                    .map_or_else(|| text.clone(), |(target, _)| target.clone())
                            },
                            |modules| modules.resolve(file, &text),
                        );
                        if self.functions.contains_key(&target) {
                            pending.push(target);
                        }
                    }
                }
                nodes.extend(node.named_children(&mut node.walk()));
            }
            if found {
                break;
            }
        }
        self.list_factories.insert(name.to_owned(), found);
        found
    }
}
