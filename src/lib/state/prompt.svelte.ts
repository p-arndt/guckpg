import { PendingRequest } from './pending.svelte.js';

export interface PromptRequest {
	title: string;
	label: string;
	initial?: string;
	confirmLabel?: string;
}

/** A promise-based prompt() backed by the app's own dialog; null means cancelled. */
class Prompt extends PendingRequest<PromptRequest, string | null> {
	constructor() {
		super(null);
	}

	/** Blank answers count as cancelled. */
	override answer(value: string | null) {
		super.answer(value?.trim() || null);
	}
}

export { Prompt };
export const prompt = new Prompt();
