from dateutil.easter import easter as relay

def renamed_entry():
    return record_value(relay(emit_value(), 3))
