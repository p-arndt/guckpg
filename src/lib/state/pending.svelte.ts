/**
 * One question at a time for a dialog, answered through a promise. A new
 * question declines the open one, unless `ask` is told to join it.
 */
export class PendingRequest<Q, A> {
	#current = $state.raw<{ question: Q; resolvers: ((answer: A) => void)[] } | null>(null);
	readonly #declined: A;

	/** `declined` is the answer an open question gets when another replaces it. */
	constructor(declined: A) {
		this.#declined = declined;
	}

	get pending(): Q | null {
		return this.#current?.question ?? null;
	}

	/** `joins` returning true for the open question shares its answer instead of replacing it. */
	ask(question: Q, joins?: (open: Q) => boolean): Promise<A> {
		const current = this.#current;
		if (current && joins?.(current.question)) {
			return new Promise((resolve) => current.resolvers.push(resolve));
		}
		this.#settle(this.#declined);
		return new Promise((resolve) => (this.#current = { question, resolvers: [resolve] }));
	}

	answer(answer: A) {
		this.#settle(answer);
	}

	#settle(answer: A) {
		const current = this.#current;
		this.#current = null;
		for (const resolve of current?.resolvers ?? []) resolve(answer);
	}
}
