use super::*;

pub(super) fn contract() -> Value {
    json!({"schema":"fr-scalar-completion",
        "finally":"Capture values before cleanup; cleanup may replace exits; nest separately.",
        "except":"Ordered unbound builtin handlers; explicit raises only.",
        "classes":["ValueError","TypeError","Exception","BaseException"],
        "reraise":"Lexically active known error.",
        "local_call_raises":true,
        "block_limit":512,"implicit_exceptions":false})
}

pub(super) fn join_pending(into: &mut Environment, from: &Environment) -> bool {
    let mut changed = false;
    for (slot, value) in &from.pending_returns {
        if let Some(previous) = into.pending_returns.get_mut(slot) {
            let count = previous.flow.len();
            merge(&mut previous.flow, value.flow.clone());
            changed |= previous.flow.len() != count;
            if previous.plain_string && !value.plain_string {
                previous.plain_string = false;
                changed = true;
            }
        } else {
            into.pending_returns.insert(*slot, value.clone());
            changed = true;
        }
    }
    for (slot, value) in &from.pending_raises {
        if let Some(previous) = into.pending_raises.get_mut(slot) {
            changed |= exceptions::join(previous, value.clone());
        } else {
            into.pending_raises.insert(*slot, value.clone());
            changed = true;
        }
    }
    changed
}
