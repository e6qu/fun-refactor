// expect: fails

type IO<T> = { readonly run: () => T };

function fetchGreeting(): IO<string> {
  return { run: () => "payload" };
}

function send(payload: string): string {
  return `sent ${payload}`;
}

export const delivery = send(fetchGreeting());
