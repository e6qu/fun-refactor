# expect: passes
# title: greeting, can_vote and summary all re-check the user dict
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
