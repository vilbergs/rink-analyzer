import type { Grade } from "../lesson/types";

/** One graded step answered. Rows are only ever added. */
export type Attempt = { stepId: string; lessonId: string; grade: Grade; answeredAt: string };

/**
 * Where progress lives. Today the browser; an account backend is a second
 * implementation, and the lesson player never knows which one it has.
 */
export interface ProgressStore {
  attempts(): Promise<Attempt[]>;
  record(attempt: Attempt): Promise<void>;
  /** Lesson id to the ISO date it was last finished. */
  studied(): Promise<Record<string, string>>;
  markStudied(lessonId: string, at: string): Promise<void>;
  exportJson(): Promise<string>;
}

type StudiedRow = { lessonId: string; at: string };

const ATTEMPTS = "attempts";
const STUDIED = "studied";

export class IndexedDbProgressStore implements ProgressStore {
  #db: Promise<IDBDatabase> | null = null;

  async attempts(): Promise<Attempt[]> {
    return this.#run(ATTEMPTS, "readonly", (s) => s.getAll());
  }

  async record(attempt: Attempt): Promise<void> {
    await this.#run(ATTEMPTS, "readwrite", (s) => s.add(attempt));
  }

  async studied(): Promise<Record<string, string>> {
    const rows: StudiedRow[] = await this.#run(STUDIED, "readonly", (s) => s.getAll());
    return Object.fromEntries(rows.map((r) => [r.lessonId, r.at]));
  }

  async markStudied(lessonId: string, at: string): Promise<void> {
    await this.#run(STUDIED, "readwrite", (s) => s.put({ lessonId, at } satisfies StudiedRow));
  }

  async exportJson(): Promise<string> {
    const [attempts, studied] = await Promise.all([this.attempts(), this.studied()]);
    return JSON.stringify({ schema: 1, exported_at: new Date().toISOString(), studied, attempts }, null, 2);
  }

  #open(): Promise<IDBDatabase> {
    this.#db ??= new Promise((resolve, reject) => {
      const req = indexedDB.open("rink-replay", 1);
      req.onupgradeneeded = () => {
        req.result.createObjectStore(ATTEMPTS, { autoIncrement: true });
        req.result.createObjectStore(STUDIED, { keyPath: "lessonId" });
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    return this.#db;
  }

  async #run<T>(store: string, mode: IDBTransactionMode, op: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
    const db = await this.#open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(store, mode);
      const req = op(tx.objectStore(store));
      tx.oncomplete = () => resolve(req.result);
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
  }
}
