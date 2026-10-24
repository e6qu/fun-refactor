use super::*;

pub(super) fn contract() -> Value {
    json!({"schema":"fr-scalar-completion",
        "finally":"Run cleanup for normal flow, return, explicit raise, break and continue; cleanup can replace the pending completion.",
        "values":"Evaluate a return or raise value before cleanup. Keep separate pending values for nested protected regions.",
        "calls":"Route explicit raises from local function summaries through the caller's cleanup before leaving the function.",
        "boundary":"No exception matching, bare reraises, exception groups, implicit runtime exceptions, context managers or shared-object effects.",
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
            let count = previous.len();
            merge(previous, value.clone());
            changed |= previous.len() != count;
        } else {
            into.pending_raises.insert(*slot, value.clone());
            changed = true;
        }
    }
    changed
}
