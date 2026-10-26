import { api, type CompletionTable, type Relation } from '#lib/api.js';
import { connections } from './connections.svelte.js';

/**
 * Catalog lookups shared by the tree, query tabs and the command palette.
 * Answers are cached per connection and tree revision, so `connections.refresh`
 * (also done on disconnect) makes every reader fetch again, and older revisions
 * are dropped then.
 * Reading through the catalog inside an effect tracks the revision.
 */
export class Catalog {
	#cache = new Map<string, Promise<unknown>>();
	readonly #revision: (connectionId: string) => number;

	constructor(revision: (connectionId: string) => number) {
		this.#revision = revision;
	}

	databases(connectionId: string, fresh = false): Promise<string[]> {
		return this.#get(connectionId, ['databases'], fresh, () => api.listDatabases(connectionId));
	}

	schemas(
		connectionId: string,
		database: string,
		includeSystem = false,
		fresh = false
	): Promise<string[]> {
		return this.#get(connectionId, ['schemas', database, String(includeSystem)], fresh, () =>
			api.listSchemas(connectionId, database, includeSystem)
		);
	}

	relations(
		connectionId: string,
		database: string,
		schema: string,
		fresh = false
	): Promise<Relation[]> {
		return this.#get(connectionId, ['relations', database, schema], fresh, () =>
			api.listRelations(connectionId, database, schema)
		);
	}

	completionTables(
		connectionId: string,
		database: string,
		fresh = false
	): Promise<CompletionTable[]> {
		return this.#get(connectionId, ['completion', database], fresh, () =>
			api.completionTables(connectionId, database)
		);
	}

	#get<T>(
		connectionId: string,
		parts: string[],
		fresh: boolean,
		fetch: () => Promise<T>
	): Promise<T> {
		const revision = this.#revision(connectionId);
		const prefix = `${connectionId}\u0000`;
		const key = [connectionId, revision, ...parts].join('\u0000');
		if (!fresh) {
			const hit = this.#cache.get(key);
			if (hit) return hit as Promise<T>;
		}
		const stale = `${prefix}${revision}\u0000`;
		for (const k of [...this.#cache.keys()]) {
			if (k.startsWith(prefix) && !k.startsWith(stale)) this.#cache.delete(k);
		}
		const promise = fetch();
		this.#cache.set(key, promise);
		// Failures are not cached: the next reader tries again.
		promise.catch(() => {
			if (this.#cache.get(key) === promise) this.#cache.delete(key);
		});
		return promise;
	}
}

export const catalog = new Catalog((id) => connections.revision[id] ?? 0);
