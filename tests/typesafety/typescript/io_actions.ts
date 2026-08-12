// expect: passes
// `IO` wraps a computation without running it. `andThen` composes a larger
// description, `retry` wraps any description in a policy, and nothing touches
// the network until `run`. The retry logic exists once, as a combinator.

type IO<T> = { readonly run: () => T };

function of<T>(value: T): IO<T> {
  return { run: () => value };
}

function andThen<T, U>(action: IO<T>, step: (value: T) => IO<U>): IO<U> {
  return { run: () => step(action.run()).run() };
}

function retry<T>(times: number, action: IO<T>): IO<T> {
  return {
    run: () => {
      let failures = 0;
      for (;;) {
        try {
          return action.run();
        } catch (error) {
          failures += 1;
          if (failures >= times) throw error;
        }
      }
    },
  };
}

// A connection that fails twice and then answers, so the retry is observable.
let calls = 0;

function flakyFetch(): string {
  calls += 1;
  if (calls < 3) throw new Error("try again");
  return "payload";
}

export const greeting = andThen(retry(3, { run: flakyFetch }), (text) => of(text.toUpperCase()));
// Nothing has run yet; greeting.run() answers "PAYLOAD" on the third call.
// The Python twin runs these assertions in CI.
