from dateutil.easter import easter as relay

def entry():
    return record_value(relay(emit_value(), 3))
