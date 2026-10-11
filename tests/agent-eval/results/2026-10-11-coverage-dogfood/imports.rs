{
    let from = if node.kind() == "import_from_statement" {
        let module = node.child_by_field_name("module_name")?;
        let text: String = source[module.byte_range()]
            .replace("\\\r\n", "")
            .replace("\\\n", "")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let levels = text.chars().take_while(|c| *c == '.').count();
        let tail = &text[levels..];
        if !module_name(tail) && !(levels > 0 && tail.is_empty()) {
            return None;
        }
        if levels == 0 {
            Some(text)
        } else {
            let mut parent: Vec<_> = file
                .parent()?
                .iter()
                .map(|p| p.to_str())
                .collect::<Option<_>>()?;
            if levels > parent.len() {
                return None;
            }
            parent.truncate(parent.len() + 1 - levels);
            if !tail.is_empty() {
                parent.push(tail);
            }
            Some(parent.join("."))
        }
    } else {
        None
    };
    if from.as_ref().is_some_and(|name| !module_name(name)) {
        return None;
    }
    let mut result = Vec::new();
    for child in node.children_by_field_name("name", &mut node.walk()) {
        let (name, alias) = if child.kind() == "aliased_import" {
            (
                child.child_by_field_name("name")?,
                child.child_by_field_name("alias")?,
            )
        } else {
            (child, child)
        };
        let name: String = source[name.byte_range()]
            .replace("\\\r\n", "")
            .replace("\\\n", "")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let explicit_alias = child.kind() == "aliased_import";
        let alias = if explicit_alias {
            &source[alias.byte_range()]
        } else {
            name.split('.').next()?
        };
        if !module_name(&name) || from.is_some() && !identifier(&name) || !identifier(alias) {
            return None;
        }
        result.push((
            alias.into(),
            Binding {
                module: from.clone().unwrap_or_else(|| name.clone()),
                member: from.as_ref().map(|_| name.clone()),
                prefix: if explicit_alias || from.is_some() {
                    alias.into()
                } else {
                    name
                },
                target: None,
                ambiguous: false,
            },
        ));
    }
    (!result.is_empty()).then_some(result)
}