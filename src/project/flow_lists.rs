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
struct Element {
    flow: Flow,
    references: References,
}

impl Element {
    fn join(&mut self, other: &Self) -> bool {
        let before = self.flow.len();
        merge(&mut self.flow, other.flow.clone());
        self.references.join(&other.references) | (before != self.flow.len())
    }
}

#[derive(Clone, Default)]
pub(super) struct Lists {
    pub bindings: BTreeMap<String, BTreeSet<String>>,
    uncertain: BTreeSet<String>,
    objects: BTreeMap<String, List>,
    expressions: BTreeMap<(PathBuf, usize), References>,
}

#[derive(Clone)]
struct List {
    items: Vec<Element>,
    minimum: usize,
}

struct IndexedList {
    slots: BTreeMap<String, BTreeSet<usize>>,
    receiver: Flow,
}

fn through_alias(mut current: Flow, prior: &Flow) -> Flow {
    // Keep the alias route only for origins still present in the object. An old
    // binding snapshot must never restore values removed by a later mutation.
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
    pub fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
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

fn integer_index(node: Node<'_>, source: &str) -> Option<i64> {
    let node = calls::unparenthesized(node)?;
    if node.kind() == "unary_operator" {
        let operator = node.child_by_field_name("operator")?;
        let value = integer_index(node.child_by_field_name("argument")?, source)?;
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
    i64::from_str_radix(digits, radix).ok()
}

fn list_slot(index: i64, length: usize) -> Option<usize> {
    let slot = if index < 0 {
        (length as i64).checked_add(index)?
    } else {
        index
    };
    usize::try_from(slot).ok().filter(|slot| *slot < length)
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
        if refs.ids.is_empty() {
            env.lists.borrow_mut().expressions.remove(&key);
        } else {
            env.lists.borrow_mut().expressions.insert(key, refs);
        }
    }

    fn list_element(&self, node: Node<'t>, flow: Flow, env: &Environment) -> Element {
        let references = self.list_references(node, env);
        Element {
            flow: if references.scalar { flow } else { Flow::new() },
            references,
        }
    }

    fn element_flow(&self, value: &Element, env: &Environment) -> Flow {
        let mut flow = value.flow.clone();
        merge(
            &mut flow,
            env.lists.borrow().contents(&value.references.ids),
        );
        flow
    }

    pub(super) fn list_literal(&mut self, node: Node<'t>, env: &Environment) -> Flow {
        let items = children(node);
        if items.len() > ITEMS || env.lists.borrow().objects.len() >= ITEMS {
            self.cutoff("list-object-budget");
            return Flow::new();
        }
        let mut values = Vec::new();
        for item in items {
            if item.kind() == "list_splat" {
                self.cutoff("list-unpacking-unchecked");
            }
            let flow = self.expression(item, env);
            if !self.normal {
                return Flow::new();
            }
            values.push(self.list_element(item, flow, env));
        }
        let id = format!(
            "{}:{}:{}",
            self.list_sites.join("/"),
            self.file.display(),
            node.start_byte()
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
            },
        );
        let ids = BTreeSet::from([id]);
        let flow = env.lists.borrow().contents(&ids);
        self.record_list_references(node, env, References { ids, scalar: false });
        self.extend(flow, node, "list-allocation")
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
        if matches!(right.kind(), "assignment" | "tuple" | "expression_list") {
            return false;
        }
        let name = self.text(left).to_owned();
        let flow = self.expression(right, env);
        if !self.normal {
            return true;
        }
        let refs = self.list_references(right, env);
        let plain = self.plain_string(right, env);
        if refs.ids.is_empty() {
            if !self.tick(left, "definition") {
                return true;
            }
            env.lists.borrow_mut().bindings.remove(&name);
            env.lists.borrow_mut().uncertain.remove(&name);
        } else {
            env.lists
                .borrow_mut()
                .bindings
                .insert(name.clone(), refs.ids);
            if refs.scalar {
                env.lists.borrow_mut().uncertain.insert(name.clone());
            } else {
                env.lists.borrow_mut().uncertain.remove(&name);
            }
        }
        env.values
            .insert(name.clone(), self.extend(flow, left, "definition"));
        env.defined.insert(name.clone());
        if plain {
            env.plain_strings.insert(name);
        } else {
            env.plain_strings.remove(&name);
        }
        true
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
        let Some(index_value) = integer_index(index, self.source) else {
            self.cutoff("list-index-unchecked");
            return Some(IndexedList {
                slots: BTreeMap::new(),
                receiver,
            });
        };
        self.expression(index, env);
        let mut selected = BTreeMap::new();
        for id in ids {
            let heap = env.lists.borrow();
            let Some(list) = heap.objects.get(&id) else {
                self.cutoff("list-index-may-raise");
                continue;
            };
            let mut slots = BTreeSet::new();
            for length in list.minimum..=list.items.len() {
                if let Some(slot) = list_slot(index_value, length) {
                    slots.insert(slot);
                } else {
                    self.cutoff("list-index-may-raise");
                }
            }
            selected.insert(id, slots);
        }
        Some(IndexedList {
            slots: selected,
            receiver,
        })
    }

    pub(super) fn list_item(&mut self, node: Node<'t>, env: &Environment) -> Option<Flow> {
        let selected = self.indexed_list(node, env)?;
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
        let strong = selected.len() == 1 && selected.values().all(|slots| slots.len() == 1);
        let mut heap = env.lists.borrow_mut();
        for (id, slots) in selected {
            if let Some(list) = heap.objects.get_mut(&id) {
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
        self.expression(object, env);
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
        let method = self
            .text(function.child_by_field_name("attribute")?)
            .to_owned();
        let arguments = children(node.child_by_field_name("arguments")?);
        let (minimum, maximum) = match method.as_str() {
            "append" | "extend" => (1, 1),
            "clear" | "reverse" => (0, 0),
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
                            self.cutoff("list-index-may-raise");
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
        arguments: &[Flow],
        names: &[Option<String>],
        call: Node<'t>,
        env: &Environment,
    ) -> Option<Flow> {
        let values = children(call.child_by_field_name("arguments")?);
        let refs: Vec<_> = values
            .iter()
            .map(|value| {
                let value = if value.kind() == "keyword_argument" {
                    value.child_by_field_name("value")
                } else {
                    Some(*value)
                };
                value
                    .map(|value| self.list_references(value, env))
                    .unwrap_or_default()
            })
            .collect();
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
        if arguments.len() != values.len()
            || values
                .iter()
                .any(|n| matches!(n.kind(), "list_splat" | "dictionary_splat"))
        {
            self.cutoff("list-call-signature-unchecked");
            return Some(Flow::new());
        }
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
        for (parameter, slot) in parameters.into_iter().zip(slots) {
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
        );
        self.list_sites
            .push(format!("{}:{}", self.file.display(), call.start_byte()));
        (self.file, self.source) = (file, source);
        let result = self.invoke_with_lists(name, function, arguments, heap);
        self.list_sites.pop();
        let references = self.return_references.clone();
        env.lists.borrow_mut().objects = self.returned_lists.objects.clone();
        (
            self.file,
            self.source,
            self.returned_lists,
            self.return_references,
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
                if node.kind() == "list" {
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
