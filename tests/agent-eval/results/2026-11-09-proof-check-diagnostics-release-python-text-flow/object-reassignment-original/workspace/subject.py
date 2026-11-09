class Other:
    def lower(self):
        return 0
def entry():
    label = str(read_user())
    label = Other()
    return send(label.lower())
