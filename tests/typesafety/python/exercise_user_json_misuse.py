# expect: fails
# title: A raw dictionary where a User is required, rejected by the checker
# misuse-of: exercise_user_json_solution
from dataclasses import dataclass


@dataclass(frozen=True)
class User:
    name: str
    age: int


def greeting(user: User) -> str:
    return f"hello {user.name}"


hello = greeting({"name": "Ada", "age": 36})
