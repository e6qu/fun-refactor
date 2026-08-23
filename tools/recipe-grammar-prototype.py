#!/usr/bin/env python3
"""A throwaway parser for the recipe language, used to design it.

This is a design artifact, not shipped code. It exists because a grammar in a
document is a claim, and the cheapest way to find out whether a claim is true is to
run it. It found three inputs that parsed happily and should not have — see the
"grammar is not the whole story" section of RECIPES.md — which changed the design
before any of it was built.

Delete this when the real parser lands in `src/recipe/`.

    python3 tools/recipe-grammar-prototype.py
"""
import re, sys

STEPS = {"rename","delete","move","imports","inline","extract","signature",
         "remove-flag","restructure","rewrite"}
DIRECTIVES = {"description","requires","expect"}
BLOCK = {"recipe","schema"}
MODIFIERS = {"on-refusal","limit","allow-empty"}
KEYWORDS = STEPS | DIRECTIVES | BLOCK | MODIFIERS | {"where","to","at","as"}
PREDICATES = {"name","kind","exported","annotated-with","file","lang","in","unused",
              "duplicated","calls","called-by","implements","matches","changed"}

TOKEN = re.compile(r"""
    (?P<ws>\s+|\#[^\n]*)
  | (?P<str>"(?:[^"\\]|\\.)*"|'[^']*')
  | (?P<arrow>=>)
  | (?P<cmp><=|>=)
  | (?P<op>[=~!<>{}])
  | (?P<int>\d+)
  | (?P<ident>[a-z][a-z0-9-]*)
""", re.VERBOSE)

def lex(src):
    out, i, line = [], 0, 1
    while i < len(src):
        m = TOKEN.match(src, i)
        if not m:
            raise SyntaxError(f"line {line}: cannot read {src[i:i+16]!r}")
        line += src[i:m.end()].count("\n")
        i = m.end()
        kind = m.lastgroup
        if kind == "ws":
            continue
        out.append((kind, m.group(), line))
    return out

class P:
    def __init__(self, toks): self.t, self.i = toks, 0
    def peek(self): return self.t[self.i] if self.i < len(self.t) else ("eof","",0)
    def next(self): tok = self.peek(); self.i += 1; return tok
    def want(self, text):
        k, v, l = self.next()
        if v != text: raise SyntaxError(f"line {l}: expected {text!r}, found {v!r}")
        return v
    def at_statement_boundary(self):
        k, v, _ = self.peek()
        return k == "eof" or v == "}" or v in STEPS or v in DIRECTIVES

def parse(src):
    p = P(lex(src))
    p.want("schema"); schema = int(p.next()[1])
    recipes = []
    while p.peek()[0] != "eof":
        p.want("recipe")
        name = p.next()[1]
        p.want("{")
        directives = []
        while p.peek()[1] != "}":
            directives.append(statement(p))
        p.want("}")
        recipes.append({"name": name, "body": directives})
    return {"schema": schema, "recipes": recipes}

def statement(p):
    k, v, line = p.next()
    if v == "description": return {"description": p.next()[1]}
    if v == "requires":
        what = p.next()[1]; return {"requires": (what, p.next()[1])}
    if v == "expect":
        what = p.next()[1]
        rest = []
        while not p.at_statement_boundary():
            rest.append(p.next()[1])
        return {"expect": (what, rest)}
    if v in STEPS:
        step = {"op": v, "args": [], "where": [], "mods": []}
        # Positional arguments, up to `where` / a modifier / a boundary.
        while True:
            k2, v2, _ = p.peek()
            if v2 == "where" or v2 in MODIFIERS or p.at_statement_boundary(): break
            step["args"].append(p.next()[1])
        if p.peek()[1] == "where":
            p.next()
            while True:
                k2, v2, l2 = p.peek()
                if v2 in MODIFIERS or p.at_statement_boundary(): break
                if v2 == "!":
                    p.next(); key = p.next()[1]
                    check_predicate(key, l2); step["where"].append(("!", key)); continue
                key = p.next()[1]
                check_predicate(key, l2)
                if p.peek()[1] in ("=", "~"):
                    op = p.next()[1]; step["where"].append((key, op, p.next()[1]))
                else:
                    step["where"].append((key, True))
        while p.peek()[1] in MODIFIERS:
            mod = p.next()[1]
            if mod == "allow-empty": step["mods"].append((mod,))
            else: step["mods"].append((mod, p.next()[1]))
        return step
    raise SyntaxError(f"line {line}: {v!r} is not a step or directive")

def check_predicate(key, line):
    if key not in PREDICATES:
        near = [c for c in PREDICATES if c.startswith(key[:3])]
        hint = f" — did you mean {near[0]!r}?" if near else ""
        raise SyntaxError(f"line {line}: unknown predicate {key!r}{hint}")

EXAMPLES = {
"retire-legacy-auth": '''
schema 1
recipe retire-legacy-auth {
  description "The legacy auth path has been dark for a year."
  requires symbol "USE_LEGACY_AUTH"
  remove-flag "USE_LEGACY_AUTH" = false
  delete where kind=function
               name~"legacy_auth_*"
               !exported
        on-refusal report
  imports where changed
  expect no-new unused
}''',
"formatting": '''
schema 1
recipe no-legacy-string-formatting {
  restructure python '"%s" % ($X,)' => 'f"{$X}"'
  expect no-new unused
}''',
"rename": '''
schema 1
recipe rename-parse-url {
  requires symbol "parse_url"
  rename to "parse_uri" where name="parse_url" kind=function
  imports where changed
  expect refusals = 0
}''',
"adapters": '''
schema 1
recipe drop-dead-adapters {
  delete where unused !exported in="src/adapters/"
         on-refusal allow
  expect changed > 0 files
}''',
"every step": '''
schema 1
recipe every-step {
  rename to "b" where name="a"
  delete where unused
  move to "src/x.rs" where name="h"
  imports where changed
  inline variable where name="l"
  inline call where name="s"
  signature "add:1:t: int:30" where name="f"
  remove-flag "F" = false
  restructure python 'a($X)' => 'b($X)'
  rewrite guard-clause where lang=go in="pkg/"
  extract function at "r.py:24:5-31:20" as "acc"
}''',
}

BAD = {
  "typo in predicate": 'schema 1\nrecipe r { delete where exportd }',
  "typo in step":      'schema 1\nrecipe r { delte where unused }',
  "no schema":         'recipe r { delete where unused }',
}

ok = True
for name, src in EXAMPLES.items():
    try:
        tree = parse(src)
        body = tree["recipes"][0]["body"]
        steps = [d for d in body if "op" in d]
        print(f"  ok   {name}: {len(body)} directives, {len(steps)} steps")
        for s in steps:
            print(f"         {s['op']:12} args={s['args']} where={s['where']} mods={s['mods']}")
    except SyntaxError as e:
        ok = False
        print(f"  FAIL {name}: {e}")

print("\nerror messages")
for name, src in BAD.items():
    try:
        parse(src); print(f"  FAIL {name}: parsed, but should not have"); ok = False
    except SyntaxError as e:
        print(f"  ok   {name}: {e}")


print("\nadversarial")
ADVERSARIAL = {
 "missing value before next step":
   'schema 1\nrecipe r {\n  delete where name=\n  imports where changed\n}',
 "step missing its required argument":
   'schema 1\nrecipe r {\n  rewrite where lang=go\n}',
 "rename with no target":
   'schema 1\nrecipe r {\n  rename where name="a"\n}',
 "value that is a step keyword":
   'schema 1\nrecipe r {\n  delete where kind=delete\n}',
 "selector on an operation that takes none":
   'schema 1\nrecipe r {\n  remove-flag "F" = false where unused\n}',
 "two recipes in one file":
   'schema 1\nrecipe a { delete where unused }\nrecipe b { imports where changed }',
 "modifier before where":
   'schema 1\nrecipe r {\n  delete on-refusal allow where unused\n}',
 "unterminated raw string":
   "schema 1\nrecipe r {\n  restructure python 'a($X) => 'b'\n}",
}
for name, src in ADVERSARIAL.items():
    try:
        tree = parse(src)
        steps = [d for r in tree["recipes"] for d in r["body"] if "op" in d]
        print(f"  parsed  {name}: {[(s['op'], s['args'], s['where']) for s in steps]}")
    except SyntaxError as e:
        print(f"  refused {name}: {e}")

sys.exit(0 if ok else 1)
