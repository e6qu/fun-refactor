# expect: passes
# title: Json travels through every signature, and every use needs a cast
from typing import cast

type Json = None | bool | int | float | str | list["Json"] | dict[str, "Json"]


def line_total(line: Json) -> Json:
    fields = cast(dict[str, Json], line)
    return cast(int, fields["pence"]) * cast(int, fields["quantity"])


def invoice_total(lines: Json) -> Json:
    rows = cast(list[Json], lines)
    return sum(cast(int, line_total(row)) for row in rows)


def is_large(lines: Json) -> bool:
    return cast(int, invoice_total(lines)) > 1000
