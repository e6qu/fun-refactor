pub fn framework_emitted(total: usize, limit: usize) -> usize {
    total.min(limit)
}

pub fn application_adapter_reads(adapter: usize, feature: usize) -> bool {
    (feature <= 1 && adapter <= 3)
        || (feature == 2 && adapter == 1)
        || (feature == 3 && (adapter == 0 || adapter == 4))
}

pub fn application_adapter_writes(adapter: usize, feature: usize) -> bool {
    (feature <= 2 && adapter <= 3) || (feature == 3 && (adapter == 0 || adapter == 4))
}

pub fn application_adapter_supports(adapter: usize, feature: usize) -> bool {
    application_adapter_reads(adapter, feature) && application_adapter_writes(adapter, feature)
}

pub fn application_adapters_compatible(source: usize, target: usize, feature: usize) -> bool {
    source != target
        && application_adapter_reads(source, feature)
        && application_adapter_writes(target, feature)
}

pub fn application_json_status_admitted(status: usize) -> bool {
    (200..=599).contains(&status) && status != 204 && status != 205 && status != 304
}

pub fn application_request_input_admitted(method: usize, source: usize, scalar: usize) -> bool {
    method <= 5 && scalar <= 2 && (source == 0 || source == 1 && (1..=3).contains(&method))
}

pub fn application_fastapi_input_admitted(
    source: usize,
    scalar: usize,
    alias_safe: bool,
    embedded: bool,
    extra_metadata: bool,
) -> bool {
    source <= 1
        && scalar <= 2
        && alias_safe
        && !extra_metadata
        && (source == 0 && !embedded || source == 1 && embedded)
}

pub fn application_validated_endpoint_agreement(
    method: bool,
    path: bool,
    inputs: bool,
    status: bool,
    response: bool,
) -> bool {
    method && path && inputs && status && response
}

pub fn application_dispositions_complete(
    input: usize,
    assigned: usize,
    unique: bool,
    exact_ids: bool,
) -> bool {
    input == assigned && unique && exact_ids
}

pub fn application_endpoint_agreement(
    method: bool,
    path: bool,
    status: bool,
    response: bool,
) -> bool {
    method && path && status && response
}

pub fn application_static_resources_admitted(
    nodes: usize,
    depth: usize,
    encoded_bytes: usize,
) -> bool {
    (1..=1024).contains(&nodes) && depth <= 32 && encoded_bytes <= 1_048_576
}

pub fn framework_omitted(total: usize, limit: usize) -> usize {
    total.saturating_sub(limit)
}

pub fn middleware_request_order(total: usize, declaration_index: usize) -> usize {
    total.saturating_sub(declaration_index)
}

pub fn application_middleware_chain_admitted(
    total: usize,
    resolved: usize,
    configured: usize,
) -> bool {
    (1..=64).contains(&total) && resolved == total && configured == 0
}

pub fn application_dependency_admitted(provider_safe: bool, configured: bool) -> bool {
    provider_safe && !configured
}

pub fn component_hooks_compatible(client: bool, runtime_hooks: usize) -> bool {
    client || runtime_hooks == 0
}

pub fn standalone_react_admitted(
    react_dependency: bool,
    next_dependency: bool,
    jsx_file: bool,
    syntax_valid: bool,
    component_found: bool,
) -> bool {
    react_dependency && !next_dependency && jsx_file && syntax_valid && component_found
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

pub fn service_route_candidate(
    local_target: bool,
    path_equal: bool,
    method_known: bool,
    method_equal: bool,
) -> bool {
    local_target && path_equal && (!method_known || method_equal)
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

pub fn fastapi_body_parameter_automatic(
    candidate_count: usize,
    path_collision: bool,
    query_collision: bool,
) -> bool {
    candidate_count == 1 && !path_collision && !query_collision
}

pub fn nextjs_body_validation_automatic(candidate_count: usize, supported_shape: bool) -> bool {
    candidate_count == 1 && supported_shape
}

pub fn fastapi_registration_automatic(
    explicit_target: bool,
    application_binding: bool,
    endpoint_conflict: bool,
) -> bool {
    explicit_target && application_binding && !endpoint_conflict
}

pub fn migration_cutover_automatic(
    explicit_cutover: bool,
    registration_automatic: bool,
    external_references: bool,
) -> bool {
    explicit_cutover && registration_automatic && !external_references
}

pub fn migration_dependency_edit_automatic(
    pep621_manifest: bool,
    owns_destination: bool,
    dependencies_array: bool,
    requirements_cover_missing: bool,
) -> bool {
    pep621_manifest && owns_destination && dependencies_array && requirements_cover_missing
}
