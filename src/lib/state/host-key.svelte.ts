export interface HostKeyQuestion {
	host: string;
	port: number;
	fingerprint: string;
}

interface Pending extends HostKeyQuestion {
	resolve: (trust: boolean) => void;
}

/** Bridges a backend "unknown host key" error to the confirmation dialog. */
class HostKeyPrompt {
	pending = $state<Pending | null>(null);

	ask(question: HostKeyQuestion): Promise<boolean> {
		// A second tunnel to the same host while the dialog is open gets the
		// same answer instead of a second dialog.
		const current = this.pending;
		if (current && current.host === question.host && current.port === question.port) {
			return new Promise((resolve) => {
				const previous = current.resolve;
				current.resolve = (trust) => {
					previous(trust);
					resolve(trust);
				};
			});
		}
		current?.resolve(false);
		return new Promise((resolve) => {
			this.pending = {
				host: question.host,
				port: question.port,
				fingerprint: question.fingerprint,
				resolve
			};
		});
	}

	answer(trust: boolean) {
		const p = this.pending;
		this.pending = null;
		p?.resolve(trust);
	}
}

export const hostKeyPrompt = new HostKeyPrompt();
