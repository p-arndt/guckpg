import { PendingRequest } from './pending.svelte.js';

export interface HostKeyQuestion {
	host: string;
	port: number;
	fingerprint: string;
}

/** Bridges a backend "unknown host key" error to the confirmation dialog. */
class HostKeyPrompt extends PendingRequest<HostKeyQuestion, boolean> {
	constructor() {
		super(false);
	}

	override ask({ host, port, fingerprint }: HostKeyQuestion): Promise<boolean> {
		// A second tunnel to the same host while the dialog is open gets the
		// same answer instead of a second dialog.
		return super.ask(
			{ host, port, fingerprint },
			(open) => open.host === host && open.port === port
		);
	}
}

export { HostKeyPrompt };
export const hostKeyPrompt = new HostKeyPrompt();
