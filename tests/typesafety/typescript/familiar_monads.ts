// expect: passes
// An array holds many results, and `flatMap` is its `andThen`: apply the step
// to each value, and flatten. A `Promise` holds a later result, and `then` is
// its `andThen`. You have been using monads all along.

type Team = { readonly name: string; readonly logins: readonly string[] };

export function allLogins(teams: Team[]): string[] {
  // For each team, a list of logins; flatMap flattens the lists.
  return teams.flatMap((team) => [...team.logins]);
}

export function fetchGreeting(fetchName: (id: number) => Promise<string>): Promise<string> {
  // `then` chains a step that returns another Promise, and flattens it.
  return fetchName(7).then((name) => Promise.resolve(`hello ${name}`));
}
