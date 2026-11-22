use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Positional,
    Either,
    Keyword,
    Args,
    Kwargs,
}

#[derive(Clone)]
struct Parameter {
    name: String,
    kind: Kind,
    span: Span,
    defaulted: bool,
}

#[derive(Clone)]
pub(super) struct Signature {
    parameters: Vec<Parameter>,
    raw: Vec<Span>,
    receiver: bool,
}

fn refuse(message: &str) -> anyhow::Error {
    Refusal::Declined {
        detail: message.into(),
    }
    .into()
}

// The index compares source spellings; Python binds identifiers after NFKC
// normalization. Check every indexed Python file, since normalization can hide
// callers from the target's reference list. String data and comments are not names.
pub(super) fn checked_identifier_spellings(index: &Index) -> Result<()> {
    let mut parsers = Parsers::new();
    for (path, info) in index.files() {
        if info.language != Language::Python {
            continue;
        }
        let source = crate::vfs::read_to_string(path)?;
        if source.is_ascii() {
            continue;
        }
        let parsed = parsers.parse(Language::Python, &source)?;
        let mut pending = vec![parsed.root()];
        while let Some(node) = pending.pop() {
            if node.kind() == "identifier"
                && !unicode_normalization::is_nfkc(&source[node.byte_range()])
            {
                return Err(refuse(&format!(
                    "Python identifier normalization is not checked at {}; use normalized identifier spellings before changing a signature",
                    location(path, node.start_byte())
                )));
            }
            pending.extend(node.named_children(&mut node.walk()));
        }
    }
    Ok(())
}

impl Signature {
    pub(super) fn parse(list: Node<'_>, source: &str, kind: SymbolKind) -> Result<Self> {
        let mut pending: Vec<_> = list
            .parent()
            .and_then(|node| node.child_by_field_name("body"))
            .into_iter()
            .collect();
        while let Some(node) = pending.pop() {
            if node.kind() == "call"
                && node.child_by_field_name("function").is_some_and(|name| {
                    matches!(
                        &source[name.byte_range()],
                        "locals" | "vars" | "eval" | "exec"
                    )
                })
            {
                return Err(refuse(
                    "dynamic local inspection prevents a checked signature change",
                ));
            }
            pending.extend(node.named_children(&mut node.walk()));
        }
        let raw = list_items(list);
        let mut parameters: Vec<Parameter> = Vec::new();
        let mut keyword = false;
        for span in &raw {
            let node = list
                .named_children(&mut list.walk())
                .find(|n| Span::from(*n) == *span)
                .unwrap();
            match node.kind() {
                "positional_separator" => {
                    for p in &mut parameters {
                        p.kind = Kind::Positional;
                    }
                    continue;
                }
                "keyword_separator" => {
                    keyword = true;
                    continue;
                }
                _ => (),
            }
            let text = span.text(source).trim();
            let kind = if text.starts_with("**") {
                Kind::Kwargs
            } else if text.starts_with('*') {
                keyword = true;
                Kind::Args
            } else if keyword {
                Kind::Keyword
            } else {
                Kind::Either
            };
            let name = parameter_name(text, Language::Python)
                .ok_or_else(|| refuse("parameter binding is not established"))?;
            if !descendants(node).into_iter().any(|child| {
                child.kind() == "identifier" && source[child.byte_range()] == name
            }) {
                return Err(refuse("parameter binding is not established"));
            }
            parameters.push(Parameter {
                name,
                kind,
                span: *span,
                defaulted: node.child_by_field_name("value").is_some(),
            });
        }
        let receiver = kind == SymbolKind::Method;
        if receiver
            && !parameters
                .first()
                .is_some_and(|p| matches!(p.kind, Kind::Positional | Kind::Either))
        {
            return Err(refuse(
                "a method signature needs a fixed receiver parameter",
            ));
        }
        if receiver {
            parameters.remove(0);
        }
        if parameters.len() > 64 {
            return Err(refuse("signature binding exceeds 64 parameters"));
        }
        Ok(Self {
            parameters,
            raw,
            receiver,
        })
    }

    pub(super) fn spans(&self) -> Vec<Span> {
        self.parameters.iter().map(|p| p.span).collect()
    }

    pub(super) fn compatible(&self, other: &Self) -> bool {
        self.receiver == other.receiver
            && self.parameters.len() == other.parameters.len()
            && self
                .parameters
                .iter()
                .zip(&other.parameters)
                .all(|(a, b)| a.name == b.name && a.kind == b.kind && a.defaulted == b.defaulted)
    }

    pub(super) fn declaration(
        &self,
        edits: &mut EditSet,
        site: &Site<'_>,
        list: Node<'_>,
        change: &Change,
    ) -> Result<()> {
        match change {
            Change::Remove(at) => {
                let parameter = self
                    .parameters
                    .get(*at)
                    .ok_or_else(|| refuse("parameter position is outside the signature"))?;
                if self.raw.len() == self.parameters.len()
                    && !matches!(parameter.kind, Kind::Args | Kind::Kwargs)
                {
                    return apply_change(
                        edits,
                        site,
                        list.start_byte(),
                        &self.spans(),
                        change,
                        true,
                        None,
                    );
                }
                let mut remove = BTreeSet::from([parameter.span]);
                let remaining: Vec<_> = self
                    .parameters
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| (i != *at).then_some(p))
                    .collect();
                for span in &self.raw {
                    let text = span.text(site.source).trim();
                    if text == "/"
                        && !self.receiver
                        && !remaining.iter().any(|p| p.kind == Kind::Positional)
                        || text == "*" && !remaining.iter().any(|p| p.kind == Kind::Keyword)
                    {
                        remove.insert(*span);
                    }
                }
                if parameter.kind == Kind::Args && remaining.iter().any(|p| p.kind == Kind::Keyword)
                {
                    remove.remove(&parameter.span);
                    edits.add(
                        site.file.to_path_buf(),
                        Edit::new(parameter.span, "*", "retain keyword-only separator"),
                    );
                }
                remove_items(edits, site, list, &remove);
            }
            Change::Move { from, to } => {
                let (Some(first), Some(last)) =
                    (self.parameters.get(*from), self.parameters.get(*to))
                else {
                    return Err(refuse(
                        "parameter positions are not both present in the signature",
                    ));
                };
                if from == to {
                    return Ok(());
                }
                if first.kind != last.kind
                    || matches!(first.kind, Kind::Args | Kind::Kwargs)
                    || self.parameters[*from.min(to)..=*from.max(to)]
                        .iter()
                        .any(|p| p.kind != first.kind)
                {
                    return Err(refuse("a move must preserve positional-only, keyword-only and variadic parameter bindings"));
                }
                let mut ordered = self.parameters.clone();
                let moved = ordered.remove(*from);
                ordered.insert(*to, moved);
                let mut defaulted = false;
                for p in &ordered {
                    if !matches!(p.kind, Kind::Positional | Kind::Either) {
                        continue;
                    }
                    if defaulted && !p.defaulted {
                        return Err(refuse(
                            "moving this parameter would put a default before one without a default",
                        ));
                    }
                    defaulted |= p.defaulted;
                }
                move_items(edits, site.file, site.source, &self.spans(), *from, *to);
            }
            Change::Add { .. } => {
                return Err(refuse(
                    "adding parameters needs an explicit binding-kind choice",
                ))
            }
        }
        Ok(())
    }

    pub(super) fn call(
        &self,
        edits: &mut EditSet,
        site: &Site<'_>,
        call: Node<'_>,
        change: &Change,
        index: &Index,
        reference: &crate::model::Reference,
    ) -> Result<()> {
        if self.receiver {
            if super::super::receiver_known_type(index, reference).is_none() {
                return Err(refuse("method receiver binding is not established; unbound class calls need an explicit migration"));
            }
            if let Some(name @ ("self" | "this")) = reference.receiver.as_deref() {
                let mut pending: Vec<_> =
                    std::iter::successors(call.parent(), |node| node.parent())
                        .find(|node| node.kind() == "function_definition")
                        .and_then(|node| node.child_by_field_name("body"))
                        .into_iter()
                        .collect();
                while let Some(node) = pending.pop() {
                    if matches!(
                        node.kind(),
                        "assignment"
                            | "augmented_assignment"
                            | "named_expression"
                            | "for_statement"
                    ) {
                        let mut targets: Vec<_> = node
                            .child_by_field_name("left")
                            .or_else(|| node.child_by_field_name("name"))
                            .into_iter()
                            .collect();
                        while let Some(target) = targets.pop() {
                            if target.kind() == "identifier"
                                && &site.source[target.byte_range()] == name
                            {
                                return Err(refuse("reassigned method receivers need an explicit binding migration"));
                            }
                            if matches!(
                                target.kind(),
                                "pattern_list"
                                    | "tuple_pattern"
                                    | "list_pattern"
                                    | "list_splat_pattern"
                            ) {
                                targets.extend(target.named_children(&mut target.walk()));
                            }
                        }
                    }
                    pending.extend(node.named_children(&mut node.walk()));
                }
            }
        }
        let list = argument_list(call).ok_or_else(|| refuse("call arguments are not available"))?;
        let mut supplied = Vec::new();
        arguments(list, site.source, &mut supplied, 0)?;
        let positions: Vec<_> = self
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(i, p)| matches!(p.kind, Kind::Positional | Kind::Either).then_some(i))
            .collect();
        let varargs = self.parameters.iter().position(|p| p.kind == Kind::Args);
        let kwargs = self.parameters.iter().position(|p| p.kind == Kind::Kwargs);
        let mut slots = vec![Vec::new(); self.parameters.len()];
        let mut positional = 0;
        let mut seen = BTreeSet::new();
        let mut order: Vec<_> = (0..supplied.len()).collect();
        order.sort_by_key(|i| supplied[*i].name.is_some());
        for i in order {
            let arg = &supplied[i];
            let slot = if let Some(name) = &arg.name {
                if !seen.insert(name) {
                    return Err(refuse("cannot rewrite duplicate keyword bindings"));
                }
                self.parameters
                    .iter()
                    .position(|p| matches!(p.kind, Kind::Either | Kind::Keyword) && &p.name == name)
                    .or(kwargs)
            } else {
                let at = positions.get(positional).copied().or(varargs);
                positional += 1;
                at
            }
            .ok_or_else(|| refuse("the original call has an invalid parameter binding"))?;
            if !slots[slot].is_empty()
                && !matches!(self.parameters[slot].kind, Kind::Args | Kind::Kwargs)
            {
                return Err(refuse("the original call supplies a parameter twice"));
            }
            slots[slot].push(i);
        }
        if self.parameters.iter().zip(&slots).any(|(p, slot)| {
            slot.is_empty() && !p.defaulted && !matches!(p.kind, Kind::Args | Kind::Kwargs)
        }) {
            return Err(refuse("the original call omits a required parameter"));
        }
        if matches!(change, Change::Remove(_))
            && self.parameters.iter().all(|p| p.kind == Kind::Either)
            && supplied.iter().all(|arg| arg.container == list)
        {
            let Change::Remove(at) = change else {
                unreachable!()
            };
            return apply_change(
                edits,
                site,
                list.start_byte(),
                &list_items(list),
                change,
                false,
                Some(&self.parameters[*at].name),
            );
        }
        match change {
            Change::Remove(at) => {
                let mut groups: BTreeMap<Span, (Node<'_>, BTreeSet<Span>)> = BTreeMap::new();
                for i in &slots[*at] {
                    let arg = &supplied[*i];
                    safety::argument(site, arg.value, false)?;
                    groups
                        .entry(Span::from(arg.container))
                        .or_insert_with(|| (arg.container, BTreeSet::new()))
                        .1
                        .insert(arg.item);
                }
                for (_, (container, removed)) in groups {
                    remove_items(edits, site, container, &removed);
                }
            }
            Change::Move { from, to } if from != to => {
                if self.parameters[*from].kind == Kind::Keyword {
                    return Ok(());
                }
                let low = *from.min(to);
                let high = *from.max(to);
                let affected: Vec<_> = (low..=high)
                    .filter_map(|i| slots[i].first().copied())
                    .collect();
                if affected.is_empty() {
                    return Ok(());
                }
                let all_positional = affected.len() == high - low + 1
                    && affected.iter().all(|i| supplied[*i].name.is_none());
                if all_positional {
                    let spans: Vec<_> = affected.iter().map(|i| supplied[*i].value).collect();
                    safety::moving_arguments(site, &spans)?;
                    move_items(edits, site.file, site.source, &spans, from - low, to - low);
                } else {
                    if self.parameters[*from].kind == Kind::Positional
                        || (low..=high).any(|i| slots[i].is_empty())
                    {
                        return Err(refuse("this call omits a parameter in the moved range; its default binding needs an explicit migration"));
                    }
                    for (slot, values) in slots.iter().enumerate() {
                        if slot < low {
                            continue;
                        }
                        for i in values {
                            let arg = &supplied[*i];
                            if arg.name.is_some() {
                                continue;
                            }
                            if self.parameters[slot].kind != Kind::Either || arg.container != list {
                                return Err(refuse("expanded or variadic arguments cannot change position across keyword bindings"));
                            }
                            edits.add(
                                site.file.to_path_buf(),
                                Edit::new(
                                    Span::new(arg.value.start, arg.value.start),
                                    format!("{}=", self.parameters[slot].name),
                                    "preserve named argument binding",
                                ),
                            );
                        }
                    }
                }
            }
            Change::Move { .. } => (),
            Change::Add { .. } => {
                return Err(refuse(
                    "adding parameters needs an explicit binding-kind choice",
                ))
            }
        }
        Ok(())
    }
}

struct Argument<'t> {
    name: Option<String>,
    item: Span,
    value: Span,
    container: Node<'t>,
}

fn arguments<'t>(
    list: Node<'t>,
    source: &str,
    out: &mut Vec<Argument<'t>>,
    depth: usize,
) -> Result<()> {
    if depth > 16 {
        return Err(refuse("argument expansion exceeds its nesting budget"));
    }
    for node in list
        .named_children(&mut list.walk())
        .filter(|n| n.kind() != "comment")
    {
        match node.kind() {
            "list_splat" => {
                let mut value = node
                    .named_child(0)
                    .ok_or_else(|| refuse("expanded argument has no value"))?;
                while value.kind() == "parenthesized_expression" {
                    value = value
                        .named_child(0)
                        .ok_or_else(|| refuse("expanded argument has no value"))?;
                }
                if !matches!(value.kind(), "list" | "tuple") {
                    return Err(refuse(
                        "expanded arguments require a bounded literal list or tuple",
                    ));
                }
                arguments(value, source, out, depth + 1)?;
            }
            "dictionary_splat" => {
                let value = node
                    .named_child(0)
                    .ok_or_else(|| refuse("expanded argument has no value"))?;
                if value.kind() != "dictionary" {
                    return Err(refuse(
                        "expanded arguments with keywords require a bounded literal dictionary",
                    ));
                }
                for pair in value
                    .named_children(&mut value.walk())
                    .filter(|n| n.kind() != "comment")
                {
                    let key = pair
                        .child_by_field_name("key")
                        .ok_or_else(|| refuse("nested dictionary expansion is not established"))?;
                    let text = Span::from(key).text(source);
                    let name = text
                        .strip_prefix('\'')
                        .and_then(|s| s.strip_suffix('\''))
                        .or_else(|| text.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                        .filter(|s| s.is_ascii() && !s.contains(['\\', '\'', '"']))
                        .ok_or_else(|| {
                            refuse("expanded keyword names require plain ASCII string keys")
                        })?;
                    let argument = pair
                        .child_by_field_name("value")
                        .ok_or_else(|| refuse("expanded keyword value is missing"))?;
                    out.push(Argument {
                        name: Some(name.into()),
                        item: Span::from(pair),
                        value: Span::from(argument),
                        container: value,
                    });
                }
            }
            "keyword_argument" => {
                let name = node
                    .child_by_field_name("name")
                    .ok_or_else(|| refuse("keyword name is missing"))?;
                let value = node
                    .child_by_field_name("value")
                    .ok_or_else(|| refuse("keyword value is missing"))?;
                out.push(Argument {
                    name: Some(Span::from(name).text(source).into()),
                    item: Span::from(node),
                    value: Span::from(value),
                    container: list,
                });
            }
            _ => out.push(Argument {
                name: None,
                item: Span::from(node),
                value: Span::from(node),
                container: list,
            }),
        }
        if out.len() > 64 {
            return Err(refuse("expanded call exceeds 64 arguments"));
        }
    }
    Ok(())
}

fn remove_items(edits: &mut EditSet, site: &Site<'_>, list: Node<'_>, removed: &BTreeSet<Span>) {
    let items = list_items(list);
    let kept: Vec<_> = items
        .iter()
        .filter(|span| !removed.contains(span))
        .collect();
    for span in removed {
        edits.add(
            site.file.to_path_buf(),
            Edit::new(*span, "", "remove bound item"),
        );
    }
    let commas: Vec<_> = list
        .children(&mut list.walk())
        .filter(|n| n.kind() == ",")
        .map(Span::from)
        .collect();
    for comma in &commas {
        let previous = items.iter().rev().find(|span| span.end <= comma.start);
        if kept.is_empty() || previous.is_some_and(|span| removed.contains(span)) {
            edits.add(
                site.file.to_path_buf(),
                Edit::new(*comma, "", "remove item separator"),
            );
        }
    }
    if list.kind() == "tuple" && kept.len() == 1 {
        let last = kept[0];
        if !commas.iter().any(|comma| comma.start >= last.end) {
            edits.add(
                site.file.to_path_buf(),
                Edit::new(
                    Span::new(last.end, last.end),
                    ",",
                    "preserve singleton tuple",
                ),
            );
        }
    }
}
