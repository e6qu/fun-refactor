# expect: passes
# run: yes
# title: parse_user builds the User once, and the checks disappear downstream
# improves: exercise_user_json_start
import json
from dataclasses import dataclass


@dataclass(frozen=True)
class User:
    name: str
    age: int


def parse_user(body: str) -> User:
    data = json.loads(body)
    match data:
        case {"name": str(name), "age": int(age)}:
            return User(name, age)
        case _:
            raise ValueError(f"not a user: {body}")


def greeting(user: User) -> str:
    return f"hello {user.name}"


def can_vote(user: User) -> bool:
    return user.age >= 18


def summary(body: str) -> str:
    user = parse_user(body)
    return f"{greeting(user)}, can vote: {can_vote(user)}"


assert summary('{"name": "Ada", "age": 36}') == "hello Ada, can vote: True"
