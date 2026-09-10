pub fn framework_emitted(total: usize, limit: usize) -> usize {
    total.min(limit)
}

pub fn framework_omitted(total: usize, limit: usize) -> usize {
    total.saturating_sub(limit)
}

pub fn middleware_request_order(total: usize, declaration_index: usize) -> usize {
    total.saturating_sub(declaration_index)
}

pub fn component_hooks_compatible(client: bool, runtime_hooks: usize) -> bool {
    client || runtime_hooks == 0
}

pub fn configuration_visibility(nextjs: bool, public_name: bool) -> usize {
    if !nextjs {
        0
    } else if public_name {
        2
    } else {
        1
    }
}

pub fn service_target_kind(absolute_http: bool, root_relative: bool) -> usize {
    if absolute_http {
        2
    } else if root_relative {
        1
    } else {
        0
    }
}

pub fn service_redaction_flags(query_or_fragment: bool, credentials: bool) -> usize {
    usize::from(query_or_fragment) + 2 * usize::from(credentials)
}

pub fn fastapi_prefix_supported(empty: bool, starts_slash: bool, ends_slash: bool) -> bool {
    empty || starts_slash && !ends_slash
}

pub fn framework_migration_supported(source_fastapi: bool, target_fastapi: bool) -> bool {
    source_fastapi != target_fastapi
}

pub fn migration_disposition(gap: bool, automatic_kind: bool) -> usize {
    if gap {
        2
    } else if automatic_kind {
        0
    } else {
        1
    }
}

pub fn migration_schema_agreement(expected: &[String], generated: &[String]) -> bool {
    expected.iter().all(|shape| generated.contains(shape))
}

pub fn nextjs_registration_automatic(declares_next: bool, app_router_path: bool) -> bool {
    declares_next && app_router_path
}
