import { plural } from '#lib/format.js';
import { PendingRequest } from './pending.svelte.js';

export interface ConfirmRequest {
	title: string;
	description: string;
	confirmLabel: string;
	destructive?: boolean;
}

/** A promise-based confirm() backed by the app's own dialog. */
export const confirmation = new PendingRequest<ConfirmRequest, boolean>(false);

/** Ask before an action would throw away unsaved grid edits. */
export function confirmDiscard(count: number): Promise<boolean> {
	if (count === 0) return Promise.resolve(true);
	return confirmation.ask({
		title: `Discard ${plural(count, 'unsaved change')}?`,
		description: 'The edits in this grid have not been saved to the database yet.',
		confirmLabel: 'Discard changes',
		destructive: true
	});
}
