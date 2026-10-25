export interface PromptRequest {
	title: string;
	label: string;
	initial?: string;
	confirmLabel?: string;
}

interface Pending extends PromptRequest {
	resolve: (value: string | null) => void;
}

/** A promise-based prompt() backed by the app's own dialog; null means cancelled. */
class Prompt {
	pending = $state<Pending | null>(null);

	ask(request: PromptRequest): Promise<string | null> {
		this.pending?.resolve(null);
		return new Promise((resolve) => (this.pending = { ...request, resolve }));
	}

	answer(value: string | null) {
		const p = this.pending;
		this.pending = null;
		const trimmed = value?.trim();
		p?.resolve(trimmed ? trimmed : null);
	}
}

export const prompt = new Prompt();
