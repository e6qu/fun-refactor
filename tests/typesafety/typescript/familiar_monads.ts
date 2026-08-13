// expect: passes

type Team = { readonly name: string; readonly logins: readonly string[] };

export function allLogins(teams: readonly Team[]): string[] {
  // For each team, a list of logins; flatMap flattens the lists.
  return teams.flatMap((team) => [...team.logins]);
}

export async function fetchGreeting(fetchName: (id: number) => Promise<string>): Promise<string> {
  // `await` unwraps the Promise, exactly the flattening `then` makes explicit.
  const name = await fetchName(7);
  return `hello ${name}`;
}
