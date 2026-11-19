use super::*;

const ITEMS: usize = 64;

#[derive(Clone, Default)]
pub(super) struct Lists {
    pub bindings: BTreeMap<String, BTreeSet<String>>,
    uncertain: BTreeSet<String>,
    objects: BTreeMap<String, List>,
}

#[derive(Clone)]
struct List {
    items: Vec<Flow>,
    minimum: usize,
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
                        target.items.resize_with(list.items.len(), Flow::new);
                        changed = true;
                    }
                    for (left, right) in target.items.iter_mut().zip(&list.items) {
                        let before = left.len();
                        merge(left, right.clone());
                        changed |= before != left.len();
                    }
                }
            }
        }
        changed
    }

    fn ids(&self, node: Node<'_>, source: &str) -> BTreeSet<String> {
        calls::unparenthesized(node)
            .filter(|node| node.kind() == "identifier")
            .and_then(|node| self.bindings.get(&source[node.byte_range()]))
            .cloned()
            .unwrap_or_default()
    }

    fn contents(&self, ids: &BTreeSet<String>) -> Flow {
        let mut result = Flow::new();
        for id in ids {
            if let Some(list) = self.objects.get(id) {
                for item in &list.items {
                    merge(&mut result, item.clone());
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
    pub(super) fn list_value(&mut self, node: Node<'t>, env: &Environment) -> Option<Flow> {
        let ids = env.lists.borrow().ids(node, self.source);
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
        Some(self.extend(env.lists.borrow().contents(&ids), node, "list-read"))
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
        let aliases = env.lists.borrow().ids(right, self.source);
        if right.kind() != "list" && aliases.is_empty() {
            return false;
        }
        let name = self.text(left).to_owned();
        let ids = if right.kind() == "list" {
            let items = children(right);
            if items.len() > ITEMS || env.lists.borrow().objects.len() >= ITEMS {
                self.cutoff("list-object-budget");
                return true;
            }
            let mut values = Vec::new();
            for item in items {
                if matches!(item.kind(), "list" | "list_splat")
                    || !env.lists.borrow().ids(item, self.source).is_empty()
                {
                    self.cutoff("nested-list-identity-unchecked");
                }
                values.push(self.expression(item, env));
                if !self.normal {
                    return true;
                }
            }
            let id = format!("{}:{}", self.file.display(), right.start_byte());
            let mut heap = env.lists.borrow_mut();
            if heap.objects.contains_key(&id) {
                self.cutoff("repeated-list-allocation");
                return true;
            }
            heap.objects.insert(
                id.clone(),
                List {
                    minimum: values.len(),
                    items: values,
                },
            );
            BTreeSet::from([id])
        } else {
            self.expression(right, env);
            aliases
        };
        env.lists.borrow_mut().bindings.insert(name.clone(), ids);
        env.lists.borrow_mut().uncertain.remove(&name);
        env.values.insert(name.clone(), Flow::new());
        env.defined.insert(name.clone());
        env.plain_strings.remove(&name);
        true
    }

    fn indexed_list(
        &mut self,
        node: Node<'t>,
        env: &Environment,
    ) -> Option<BTreeMap<String, BTreeSet<usize>>> {
        let value = node.child_by_field_name("value")?;
        let ids = env.lists.borrow().ids(value, self.source);
        if ids.is_empty() {
            return None;
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
            return Some(BTreeMap::new());
        }
        let index = node.child_by_field_name("subscript")?;
        let Some(index_value) = integer_index(index, self.source) else {
            self.cutoff("list-index-unchecked");
            return Some(BTreeMap::new());
        };
        self.expression(value, env);
        self.expression(index, env);
        let mut selected = BTreeMap::new();
        for id in ids {
            let heap = env.lists.borrow();
            let Some(list) = heap.objects.get(&id) else {
                self.cutoff("list-index-may-raise");
                continue;
            };
            let mut slots = BTreeSet::new();
            // A negative index is relative to each possible length after a join.
            for length in list.minimum..=list.items.len() {
                if let Some(slot) = list_slot(index_value, length) {
                    slots.insert(slot);
                } else {
                    self.cutoff("list-index-may-raise");
                }
            }
            selected.insert(id, slots);
        }
        Some(selected)
    }

    pub(super) fn list_item(&mut self, node: Node<'t>, env: &Environment) -> Option<Flow> {
        let selected = self.indexed_list(node, env)?;
        let mut value = Flow::new();
        let heap = env.lists.borrow();
        for (id, slots) in selected {
            if let Some(list) = heap.objects.get(&id) {
                for slot in slots {
                    if let Some(item) = list.items.get(slot) {
                        merge(&mut value, item.clone());
                    }
                }
            }
        }
        Some(self.extend(value, node, "list-item"))
    }

    fn list_write(&mut self, left: Node<'t>, right: Node<'t>, env: &Environment) -> bool {
        let Some(value) = left.child_by_field_name("value") else {
            return false;
        };
        if env.lists.borrow().ids(value, self.source).is_empty() {
            return false;
        }
        let flow = self.expression(right, env);
        if !self.normal {
            return true;
        }
        if right.kind() == "list" || !env.lists.borrow().ids(right, self.source).is_empty() {
            self.cutoff("nested-list-identity-unchecked");
        }
        let Some(selected) = self.indexed_list(left, env) else {
            return true;
        };
        let strong = selected.len() == 1 && selected.values().all(|slots| slots.len() == 1);
        let mut heap = env.lists.borrow_mut();
        for (id, slots) in selected {
            if let Some(list) = heap.objects.get_mut(&id) {
                for slot in slots {
                    if let Some(item) = list.items.get_mut(slot) {
                        let flow = self.extend(flow.clone(), left, "list-write");
                        if strong {
                            *item = flow;
                        } else {
                            merge(item, flow);
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
        let ids = env.lists.borrow().ids(object, self.source);
        if ids.is_empty() {
            return None;
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
        self.expression(object, env);
        let mut values = Vec::new();
        for argument in &arguments {
            values.push(self.expression(*argument, env));
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
            if method == "insert"
                && (calls::unparenthesized(arguments[1]).is_some_and(|node| node.kind() == "list")
                    || !env.lists.borrow().ids(arguments[1], self.source).is_empty())
            {
                self.cutoff("nested-list-identity-unchecked");
            }
            let strong = ids.len() == 1;
            let mut result = Flow::new();
            for id in ids {
                let mut heap = env.lists.borrow_mut();
                let list = heap.objects.get_mut(&id)?;
                if !strong || list.minimum != list.items.len() {
                    self.cutoff("list-mutation-target-unchecked");
                    if method == "pop" {
                        for item in &list.items {
                            merge(&mut result, item.clone());
                        }
                    }
                    continue;
                }
                match method.as_str() {
                    "pop" => {
                        if let Some(slot) = list_slot(index, list.items.len()) {
                            merge(&mut result, list.items.remove(slot));
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
                        list.items
                            .insert(slot, self.extend(values[1].clone(), node, "list-write"));
                        list.minimum += 1;
                    }
                    "reverse" => list.items.reverse(),
                    _ => unreachable!(),
                }
            }
            return Some(self.extend(result, node, "list-method-result"));
        }
        let addition = if method == "extend" {
            let refs = env.lists.borrow().ids(arguments[0], self.source);
            if refs.len() != 1 {
                self.cutoff("list-extend-source-unchecked");
                return Some(Flow::new());
            }
            let heap = env.lists.borrow();
            let list = &heap.objects[refs.first().unwrap()];
            if list.minimum != list.items.len() {
                self.cutoff("list-extend-length-unchecked");
            }
            list.items.clone()
        } else {
            if method == "append"
                && (calls::unparenthesized(arguments[0]).is_some_and(|node| node.kind() == "list")
                    || !env.lists.borrow().ids(arguments[0], self.source).is_empty())
            {
                self.cutoff("nested-list-identity-unchecked");
            }
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
                list.items.extend(
                    addition
                        .iter()
                        .cloned()
                        .map(|flow| self.extend(flow, node, "list-write")),
                );
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
                    .map(|value| env.lists.borrow().ids(value, self.source))
                    .unwrap_or_default()
            })
            .collect();
        if refs.iter().all(BTreeSet::is_empty) {
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
        for (parameter, slot) in parameters.into_iter().zip(slots) {
            if let Some(ids) = slot
                .and_then(|slot| refs.get(slot))
                .filter(|ids| !ids.is_empty())
            {
                heap.bindings
                    .insert(source[parameter.node.byte_range()].to_owned(), ids.clone());
            }
        }
        let saved = (
            self.file,
            self.source,
            std::mem::take(&mut self.returned_lists),
        );
        (self.file, self.source) = (file, source);
        let result = self.invoke_with_lists(name, function, arguments, heap);
        env.lists.borrow_mut().objects = self.returned_lists.objects.clone();
        (self.file, self.source, self.returned_lists) = saved;
        Some(self.extend(result, call, "list-helper-result"))
    }
}
