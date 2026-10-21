from jdcal import is_leap as relay

def entry():
    return record_value(relay(emit_value()))
