# expect: passes
# run: yes
# title: flatMap and comprehensions are the chaining step of monads you already use
from dataclasses import dataclass


@dataclass(frozen=True)
class Team:
    name: str
    logins: tuple[str, ...]


def all_logins(teams: list[Team]) -> list[str]:
    # For each team, a list of logins; the comprehension flattens the lists.
    return [login for team in teams for login in team.logins]


teams = [Team("ops", ("ada", "bob")), Team("web", ("cid",))]
assert all_logins(teams) == ["ada", "bob", "cid"]
