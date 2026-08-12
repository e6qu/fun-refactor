// expect: passes
// Three lookups, each of which can fail. Every failure collapses into the
// same null, so the caller cannot tell what went wrong.

function findUserId(login: string): string | null {
  return login === "ada" ? "u7" : null;
}

function findCart(userId: string): string[] | null {
  return userId === "u7" ? ["book"] : null;
}

export function firstItem(login: string): string | null {
  const userId = findUserId(login);
  if (userId === null) {
    return null;
  }
  const cart = findCart(userId);
  if (cart === null) {
    return null;
  }
  return cart[0] ?? null;
}
