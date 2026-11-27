use super::*;
use lists::{Element, List, References};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Key {
    Text(String),
    Integer(i128),
    None,
}

pub(super) fn key(node: Node<'_>, source: &str) -> Option<Key> {
    let node = calls::unparenthesized(node)?;
    match node.kind() {
        "true" => Some(Key::Integer(1)),
        "false" => Some(Key::Integer(0)),
        "none" => Some(Key::None),
        "integer" | "unary_operator" => lists::integer_value(node, source).map(Key::Integer),
        "string" => {
            let text = &source[node.byte_range()];
            let quote = text.find(['\'', '"'])?;
            let prefix = &text[..quote];
            if !prefix.is_empty() && !prefix.eq_ignore_ascii_case("r") {
                return None;
            }
            let mark = text.as_bytes()[quote] as char;
            let width = if text[quote..].starts_with(&mark.to_string().repeat(3)) {
                3
            } else {
                1
            };
            let body = text.get(quote + width..text.len().checked_sub(width)?)?;
            if prefix.eq_ignore_ascii_case("r") {
                return Some(Key::Text(body.into()));
            }
            let mut out = String::new();
            let mut chars = body.chars();
            while let Some(ch) = chars.next() {
                if ch != '\\' {
                    out.push(ch);
                    continue;
                }
                out.push(match chars.next()? {
                    '\\' => '\\',
                    '\'' => '\'',
                    '"' => '"',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    'a' => '\x07',
                    'b' => '\x08',
                    'f' => '\x0c',
                    'v' => '\x0b',
                    '\n' => continue,
                    'x' => {
                        let digits: String = chars.by_ref().take(2).collect();
                        if digits.len() != 2 {
                            return None;
                        }
                        char::from_u32(u32::from_str_radix(&digits, 16).ok()?)?
                    }
                    _ => return None,
                });
            }
            Some(Key::Text(out))
        }
        _ => None,
    }
}

fn put(keys: &mut Vec<Key>, values: &mut Vec<Element>, key: Key, value: Element) {
    if let Some(slot) = keys.iter().position(|prior| *prior == key) {
        values[slot] = value;
    } else {
        keys.push(key);
        values.push(value);
    }
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn exact_mapping(
        &mut self,
        refs: &References,
        env: &Environment,
    ) -> Option<(String, List)> {
        if refs.scalar || refs.ids.len() != 1 {
            self.cutoff("dictionary-target-unchecked");
            return None;
        }
        let id = refs.ids.first()?.clone();
        let value = env.lists.borrow().objects.get(&id).cloned();
        match value {
            Some(value)
                if value.keys.is_some()
                    && !value.uncertain_shape
                    && value.minimum == value.items.len() =>
            {
                Some((id, value))
            }
            _ => {
                self.cutoff("dictionary-shape-unchecked");
                None
            }
        }
    }

    pub(super) fn allocate_mapping(
        &mut self,
        node: Node<'t>,
        keys: Vec<Key>,
        values: Vec<Element>,
        env: &Environment,
    ) -> Flow {
        let flow = self.allocate_list(node, values, env);
        let refs = self.list_references(node, env);
        if let Some(id) = refs.ids.first() {
            if let Some(value) = env.lists.borrow_mut().objects.get_mut(id) {
                value.keys = Some(keys);
            }
        }
        flow
    }

    pub(super) fn mapping_literal(&mut self, node: Node<'t>, env: &Environment) -> Flow {
        let mut keys = Vec::new();
        let mut values = Vec::new();
        for pair in lists::children(node) {
            if pair.kind() == "dictionary_splat" {
                let Some(value) = pair.named_child(0) else {
                    self.cutoff("dictionary-expansion-unchecked");
                    return Flow::new();
                };
                self.expression(value, env);
                if !self.normal {
                    return Flow::new();
                }
                let Some((_, other)) = self.exact_mapping(&self.list_references(value, env), env)
                else {
                    return Flow::new();
                };
                for (key, value) in other.keys.unwrap().into_iter().zip(other.items) {
                    put(&mut keys, &mut values, key, value);
                }
            } else {
                let (Some(key_node), Some(value_node)) = (
                    pair.child_by_field_name("key"),
                    pair.child_by_field_name("value"),
                ) else {
                    self.cutoff("dictionary-literal-unchecked");
                    return Flow::new();
                };
                self.expression(key_node, env);
                if !self.normal {
                    return Flow::new();
                }
                let flow = self.expression(value_node, env);
                if !self.normal {
                    return Flow::new();
                }
                let Some(key) = key(key_node, self.source) else {
                    self.cutoff("dictionary-key-unchecked");
                    return Flow::new();
                };
                put(
                    &mut keys,
                    &mut values,
                    key,
                    self.list_element(value_node, flow, env),
                );
            }
            if keys.len() > 64 {
                self.cutoff("dictionary-item-budget");
                return Flow::new();
            }
        }
        self.allocate_mapping(node, keys, values, env)
    }

    pub(super) fn mapping_slot(
        &mut self,
        node: Node<'t>,
        index: Node<'t>,
        refs: &References,
        env: &Environment,
        inserting: bool,
    ) -> BTreeMap<String, BTreeSet<usize>> {
        let Some((id, mut value)) = self.exact_mapping(refs, env) else {
            return BTreeMap::new();
        };
        let Some(key) = key(index, self.source) else {
            self.cutoff("dictionary-key-unchecked");
            return BTreeMap::new();
        };
        let keys = value.keys.as_mut().unwrap();
        let slot = keys.iter().position(|prior| *prior == key);
        let slot = if let Some(slot) = slot {
            slot
        } else if inserting {
            if keys.len() >= 64 {
                self.cutoff("dictionary-item-budget");
                return BTreeMap::new();
            }
            let slot = keys.len();
            keys.push(key);
            value.items.push(Element::default());
            value.minimum += 1;
            env.lists.borrow_mut().objects.insert(id.clone(), value);
            slot
        } else {
            self.sequence_error(node, env, "KeyError");
            return BTreeMap::new();
        };
        BTreeMap::from([(id, BTreeSet::from([slot]))])
    }

    pub(super) fn mapping_method(
        &mut self,
        node: Node<'t>,
        refs: &References,
        receiver: Flow,
        env: &Environment,
    ) -> Flow {
        let method = node
            .child_by_field_name("function")
            .and_then(|n| n.child_by_field_name("attribute"))
            .map(|n| self.text(n).to_owned())
            .unwrap_or_default();
        let evaluated = self.call_arguments(node, env);
        if !self.normal || !evaluated.checked {
            return Flow::new();
        }
        let Some((id, mut value)) = self.exact_mapping(refs, env) else {
            return Flow::new();
        };
        let items = &evaluated.elements;
        let count = items.len();
        let raw = node
            .child_by_field_name("arguments")
            .map(lists::children)
            .unwrap_or_default();
        if method == "update" {
            self.mapping_arguments(node, &evaluated, env, Some((id, value)));
            return Flow::new();
        } else if evaluated.names.iter().any(Option::is_some) || evaluated.expanded {
            self.cutoff("dictionary-method-arguments");
            return Flow::new();
        } else if method == "copy" && count == 0 {
            let flow = self.allocate_mapping(node, value.keys.unwrap(), value.items, env);
            return lists::through_alias(flow, &receiver);
        } else if method == "clear" && count == 0 {
            value.keys.as_mut().unwrap().clear();
            value.items.clear();
        } else if matches!(method.as_str(), "get" | "pop" | "setdefault")
            && (1..=2).contains(&count)
        {
            let Some(key) = raw.first().and_then(|n| key(*n, self.source)) else {
                self.cutoff("dictionary-key-unchecked");
                return Flow::new();
            };
            let keys = value.keys.as_mut().unwrap();
            let slot = keys.iter().position(|prior| *prior == key);
            let result = if let Some(slot) = slot {
                if method == "pop" {
                    keys.remove(slot);
                    value.items.remove(slot)
                } else {
                    value.items[slot].clone()
                }
            } else if method == "pop" && count == 1 {
                self.sequence_error(node, env, "KeyError");
                return Flow::new();
            } else {
                let result = items.get(1).cloned().unwrap_or_default();
                if method == "setdefault" {
                    if keys.len() >= 64 {
                        self.cutoff("dictionary-item-budget");
                        return Flow::new();
                    }
                    keys.push(key);
                    value.items.push(result.clone());
                }
                result
            };
            value.minimum = value.items.len();
            env.lists.borrow_mut().objects.insert(id, value);
            self.record_list_references(node, env, result.references.clone());
            env.lists.borrow_mut().expression_facts.insert(
                (self.file.to_owned(), node.start_byte()),
                result.facts.clone(),
            );
            return self.extend(
                lists::through_alias(self.element_flow(&result, env), &receiver),
                node,
                "dictionary-item",
            );
        } else {
            self.cutoff("dictionary-method-unchecked");
            return Flow::new();
        }
        value.minimum = value.items.len();
        env.lists.borrow_mut().objects.insert(id, value);
        Flow::new()
    }

    pub(super) fn mapping_arguments(
        &mut self,
        node: Node<'t>,
        args: &calls::Arguments,
        env: &Environment,
        target: Option<(String, List)>,
    ) -> Option<(Vec<Key>, Vec<Element>)> {
        if args.names.iter().filter(|name| name.is_none()).count() > 1 {
            self.sequence_error(node, env, "TypeError");
            return None;
        }
        let (id, mut mapping) = match target {
            Some((id, value)) => (Some(id), value),
            None => (
                None,
                List {
                    items: Vec::new(),
                    minimum: 0,
                    tuple: false,
                    keys: Some(Vec::new()),
                    uncertain_shape: false,
                },
            ),
        };
        for (name, value) in args.names.iter().zip(&args.elements) {
            if let Some(name) = name {
                self.mapping_put(
                    &mut mapping,
                    (Key::Text(name.clone()), value.clone()),
                    env,
                    id.as_deref(),
                )?;
            } else {
                self.mapping_merge(node, &mut mapping, value, env, id.as_deref())?;
            }
        }
        Some((mapping.keys.unwrap(), mapping.items))
    }

    fn mapping_put(
        &mut self,
        mapping: &mut List,
        entry: (Key, Element),
        env: &Environment,
        target: Option<&str>,
    ) -> Option<()> {
        let keys = mapping.keys.as_mut().unwrap();
        if keys.len() >= 64 && !keys.contains(&entry.0) {
            self.cutoff("dictionary-item-budget");
            return None;
        }
        put(keys, &mut mapping.items, entry.0, entry.1);
        mapping.minimum = mapping.items.len();
        if let Some(id) = target {
            env.lists
                .borrow_mut()
                .objects
                .insert(id.to_owned(), mapping.clone());
        }
        Some(())
    }

    fn mapping_merge(
        &mut self,
        node: Node<'t>,
        mapping: &mut List,
        source: &Element,
        env: &Environment,
        target: Option<&str>,
    ) -> Option<()> {
        let is_mapping = source.references.ids.iter().any(|id| {
            env.lists
                .borrow()
                .objects
                .get(id)
                .is_some_and(|value| value.keys.is_some())
        });
        if is_mapping {
            let (_, other) = self.exact_mapping(&source.references, env)?;
            for entry in other.keys.unwrap().into_iter().zip(other.items) {
                self.mapping_put(mapping, entry, env, target)?;
            }
        } else {
            let items = self.sequence_items(node, source, env)?;
            for item in items {
                let pair = self.sequence_items(node, &item, env)?;
                if pair.len() != 2 {
                    self.sequence_error(node, env, "ValueError");
                    return None;
                }
                let Some(key) = pair[0].facts.key.clone() else {
                    let refs = &pair[0].references;
                    let unhashable = !refs.scalar
                        && !refs.ids.is_empty()
                        && refs.ids.iter().all(|id| {
                            env.lists
                                .borrow()
                                .objects
                                .get(id)
                                .is_some_and(|value| !value.tuple)
                        });
                    if unhashable {
                        self.sequence_error(node, env, "TypeError");
                    } else {
                        self.cutoff("dictionary-key-unchecked");
                    }
                    return None;
                };
                self.mapping_put(mapping, (key, pair[1].clone()), env, target)?;
            }
        }
        Some(())
    }

    fn mapping_operand(&mut self, node: Node<'t>, env: &Environment) -> Option<List> {
        let value = self.list_element(node, Flow::new(), env);
        let known_other = value.references.ids.is_empty()
            && (value.facts.noniterable || value.facts.plain_string)
            || !value.references.scalar
                && !value.references.ids.is_empty()
                && value.references.ids.iter().all(|id| {
                    env.lists
                        .borrow()
                        .objects
                        .get(id)
                        .is_some_and(|value| value.keys.is_none())
                });
        if known_other {
            self.sequence_error(node, env, "TypeError");
            return None;
        }
        self.exact_mapping(&value.references, env)
            .map(|(_, mapping)| mapping)
    }

    pub(super) fn mapping_binary(
        &mut self,
        node: Node<'t>,
        env: &Environment,
        flow: &Flow,
    ) -> Option<Flow> {
        if !node
            .child_by_field_name("operator")
            .is_some_and(|operator| self.text(operator) == "|")
        {
            return None;
        }
        let left = node.child_by_field_name("left")?;
        let right = node.child_by_field_name("right")?;
        if ![left, right].iter().any(|operand| {
            self.list_references(*operand, env).ids.iter().any(|id| {
                env.lists
                    .borrow()
                    .objects
                    .get(id)
                    .is_some_and(|value| value.keys.is_some())
            })
        }) {
            return None;
        }
        let Some(mut left) = self.mapping_operand(left, env) else {
            return Some(Flow::new());
        };
        let Some(right) = self.mapping_operand(right, env) else {
            return Some(Flow::new());
        };
        for entry in right.keys.unwrap().into_iter().zip(right.items) {
            if self.mapping_put(&mut left, entry, env, None).is_none() {
                return Some(Flow::new());
            }
        }
        let result = self.allocate_mapping(node, left.keys.unwrap(), left.items, env);
        Some(lists::through_alias(result, flow))
    }

    pub(super) fn mapping_augmented(&mut self, node: Node<'t>, env: &mut Environment) -> bool {
        if !self.solver.enabled
            || !node
                .child_by_field_name("operator")
                .is_some_and(|op| self.text(op) == "|=")
        {
            return false;
        }
        let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) else {
            return false;
        };
        if left.kind() != "identifier" {
            return false;
        }
        let receiver = self.expression(left, env);
        let refs = self.list_references(left, env);
        if !self.normal {
            return true;
        }
        let flow = self.expression(right, env);
        if !self.normal {
            return true;
        }
        let Some((id, mut mapping)) = self.exact_mapping(&refs, env) else {
            return true;
        };
        let source = self.list_element(right, flow, env);
        self.mapping_merge(node, &mut mapping, &source, env, Some(&id));
        if self.normal {
            self.bind_element(
                left,
                &Element {
                    flow: receiver,
                    references: refs,
                    facts: Default::default(),
                },
                env,
            );
        }
        true
    }
}
