# expect: passes
# title: One parse at the door replaces the three checks
# improves: exercise_user_json_start
from pydantic import BaseModel


class User(BaseModel):
    name: str
    age: int


def greeting(user: User) -> str:
    return f"hello {user.name}"


def can_vote(user: User) -> bool:
    return user.age >= 18


def summary(body: str) -> str:
    user = User.model_validate_json(body)
    return f"{greeting(user)}, can vote: {can_vote(user)}"
