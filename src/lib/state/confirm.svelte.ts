export interface ConfirmRequest {
	title: string;
	description: string;
	confirmLabel: string;
	destructive?: boolean;
}

interface Pending extends ConfirmRequest {
	resolve: (ok: boolean) => void;
}

/** A promise-based confirm() backed by the app's own dialog. */
class Confirmation {
	pending = $state<Pending | null>(null);

	ask(request: ConfirmRequest): Promise<boolean> {
		this.pending?.resolve(false);
		return new Promise((resolve) => (this.pending = { ...request, resolve }));
	}

	answer(ok: boolean) {
		const p = this.pending;
		this.pending = null;
		p?.resolve(ok);
	}
}

export const confirmation = new Confirmation();

/** Ask before an action would throw away unsaved grid edits. */
export function confirmDiscard(count: number): Promise<boolean> {
	if (count === 0) return Promise.resolve(true);
	return confirmation.ask({
		title: `Discard ${count} unsaved change${count === 1 ? '' : 's'}?`,
		description: 'The edits in this grid have not been saved to the database yet.',
		confirmLabel: 'Discard changes',
		destructive: true
	});
}
