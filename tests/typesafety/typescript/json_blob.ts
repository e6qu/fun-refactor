// expect: passes

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

export function lineTotal(line: Json): Json {
  const fields = line as { [key: string]: Json };
  return (fields.pence as number) * (fields.quantity as number);
}

export function invoiceTotal(lines: Json): Json {
  return (lines as Json[]).reduce((sum: number, row) => sum + (lineTotal(row) as number), 0);
}

export function isLarge(lines: Json): boolean {
  return (invoiceTotal(lines) as number) > 1000;
}
