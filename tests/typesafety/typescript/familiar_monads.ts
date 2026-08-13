// expect: passes

type Team = { readonly name: string; readonly logins: readonly string[] };

export function allLogins(teams: readonly Team[]): string[] {
  return teams.flatMap((team) => [...team.logins]);
}

export async function fetchGreeting(fetchName: (id: number) => Promise<string>): Promise<string> {
  const name = await fetchName(7);
  return `hello ${name}`;
}
