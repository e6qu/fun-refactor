// expect: passes

type Team = { readonly name: string; readonly logins: readonly string[] };

export function allLogins(teams: Team[]): string[] {
  // For each team, a list of logins; flatMap flattens the lists.
  return teams.flatMap((team) => [...team.logins]);
}

export function fetchGreeting(fetchName: (id: number) => Promise<string>): Promise<string> {
  // `then` chains a step that returns another Promise, and flattens it.
  return fetchName(7).then((name) => Promise.resolve(`hello ${name}`));
}
