import { errorMessage } from '#lib/api.js';

/** The statements of a review dialog as one script, each terminated by a semicolon. */
export function joinStatements(statements: readonly string[], separator = '\n'): string {
	return statements.map((statement) => statement + ';').join(separator);
}

/**
 * The "preview SQL, review, apply" flow shared by the editors: one request runs at a time,
 * and the dialog cannot be dismissed while a request is in flight.
 */
export class SqlReview {
	busy = $state(false);
	/** Empty when the last request succeeded. */
	error = $state('');
	sql = $state<string[]>([]);
	reviewing = $state(false);

	/** Loads the statements and opens the dialog; false if busy or the preview failed. */
	async preview(load: () => Promise<string[]>): Promise<boolean> {
		if (this.busy) return false;
		this.busy = true;
		this.error = '';
		try {
			this.sql = await load();
			this.reviewing = true;
			return true;
		} catch (e) {
			this.error = errorMessage(e);
			return false;
		} finally {
			this.busy = false;
		}
	}

	/**
	 * Runs the change; `run` closes the dialog itself (via `close(true)`) once the database
	 * accepted it, so follow-up work such as reloading can still report into `error`.
	 */
	async apply(run: () => Promise<void>): Promise<boolean> {
		if (this.busy) return false;
		this.busy = true;
		this.error = '';
		try {
			await run();
			return true;
		} catch (e) {
			this.error = errorMessage(e);
			return false;
		} finally {
			this.busy = false;
		}
	}

	/** Closes the dialog unless a request is running; `force` is for `apply`'s own callback. */
	close(force = false) {
		if (force || !this.busy) this.reviewing = false;
	}
}
