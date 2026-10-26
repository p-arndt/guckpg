/**
 * Data loaded asynchronously, where only the latest request may land: a slow
 * answer to an older request must not overwrite a newer one.
 */
export class Resource<T> {
	data = $state.raw<T | null>(null);
	error = $state.raw<unknown>(null);
	loading = $state(false);
	#request = 0;
	readonly #fetch: () => Promise<T>;

	constructor(fetch: () => Promise<T>) {
		this.#fetch = fetch;
	}

	get failed(): boolean {
		return this.error !== null;
	}

	/** Resolves true when this request loaded and is still the latest one. */
	async load(): Promise<boolean> {
		const request = ++this.#request;
		this.loading = true;
		this.error = null;
		try {
			const data = await this.#fetch();
			if (request !== this.#request) return false;
			this.data = data;
			return true;
		} catch (e) {
			if (request === this.#request) this.error = e ?? new Error('Failed');
			return false;
		} finally {
			if (request === this.#request) this.loading = false;
		}
	}

	/** Forget the data and drop answers to requests still in flight. */
	reset() {
		this.#request++;
		this.data = null;
		this.error = null;
		this.loading = false;
	}
}
