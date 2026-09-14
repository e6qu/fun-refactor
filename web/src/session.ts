/** Durable browser checkpoints around the Rust session envelope. */

const KEY = "fr-playground-checkpoint-1";

export interface SessionWorkspace {
  session(): string;
}

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

interface Envelope {
  schema: "fr-playground-checkpoint-1";
  name: string;
  session: string;
}

export type Recovered<T> =
  | { checkpoint: { name: string; workspace: T }; error: null }
  | { checkpoint: null; error: string | null };

export function saveCheckpoint(
  storage: StorageLike,
  workspace: SessionWorkspace,
  name: string,
): string | null {
  try {
    const session = workspace.session();
    const answer = JSON.parse(session);
    if (answer && typeof answer === "object" && "error" in answer) {
      return String(answer.error);
    }
    const envelope: Envelope = {
      schema: "fr-playground-checkpoint-1",
      name: name.slice(0, 256),
      session,
    };
    const encoded = JSON.stringify(envelope);
    storage.setItem(KEY, encoded);
    if (storage.getItem(KEY) !== encoded) return "browser storage did not retain the checkpoint";
    return null;
  } catch (error) {
    return `browser storage refused the checkpoint: ${error instanceof Error ? error.message : error}`;
  }
}

export function loadCheckpoint<T>(
  storage: StorageLike,
  restore: (session: string) => T,
): Recovered<T> {
  const discard = (error: string): Recovered<T> => {
    try {
      storage.removeItem(KEY);
    } catch {
      // A blocked storage implementation cannot be repaired from this page.
    }
    return { checkpoint: null, error };
  };

  let encoded: string | null;
  try {
    encoded = storage.getItem(KEY);
  } catch (error) {
    return {
      checkpoint: null,
      error: `browser storage could not be read: ${error instanceof Error ? error.message : error}`,
    };
  }
  if (encoded === null) return { checkpoint: null, error: null };

  try {
    const value: unknown = JSON.parse(encoded);
    if (!value || typeof value !== "object") return discard("saved checkpoint is not an object");
    const envelope = value as Partial<Envelope>;
    if (
      envelope.schema !== "fr-playground-checkpoint-1" ||
      typeof envelope.name !== "string" ||
      envelope.name.length > 256 ||
      typeof envelope.session !== "string" ||
      Object.keys(envelope).sort().join(",") !== "name,schema,session"
    ) {
      return discard("saved checkpoint has an invalid envelope");
    }
    return {
      checkpoint: { name: envelope.name, workspace: restore(envelope.session) },
      error: null,
    };
  } catch (error) {
    return discard(`saved checkpoint was rejected: ${error instanceof Error ? error.message : error}`);
  }
}
