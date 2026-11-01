from boltons.mathutils import clamp as relay

def entry():
    return record_value(relay(emit_value(), 0, 50))
