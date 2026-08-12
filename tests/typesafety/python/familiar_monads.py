# expect: passes
# run: yes
# title: Lists and comprehensions already follow the monad shape
"""A list holds many results, and a comprehension over two lists is the
list's `and_then`: apply the step to each value, and flatten. `Optional`
narrowing plays the same role for absence, one step at a time."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Team:
    name: str
    logins: list[str]


def all_logins(teams: list[Team]) -> list[str]:
    # For each team, a list of logins; the comprehension flattens the lists.
    return [login for team in teams for login in team.logins]


teams = [Team("ops", ["ada", "bob"]), Team("web", ["cid"])]
assert all_logins(teams) == ["ada", "bob", "cid"]
