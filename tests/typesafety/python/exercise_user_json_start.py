# expect: passes
# title: Exercise 3, starting point
"""A user arrives as JSON and travels as a dict. Three functions check it,
each in its own way, and none can trust the others."""

import json


def greeting(user: dict[str, object]) -> str:
    name = user.get("name")
    if not isinstance(name, str):
        raise ValueError("name missing")
    return f"hello {name}"


def can_vote(user: dict[str, object]) -> bool:
    age = user.get("age")
    if not isinstance(age, int):
        raise ValueError("age missing")
    return age >= 18


def summary(body: str) -> str:
    user = json.loads(body)
    if not isinstance(user, dict):
        raise ValueError("not an object")
    return f"{greeting(user)}, can vote: {can_vote(user)}"
