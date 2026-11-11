import re
def entry():
    return send(re.sub('A', 'B', str(read_user())))
