import { aiApi, defaultAiConfig, type AiContext, type AiMessage, type AiTool } from '#lib/ai.js';
import { errorMessage } from '#lib/api.js';

export class AiChat {
	settings = $state({ config: defaultAiConfig(), hasApiKey: false, hasBasicAuth: false });
	loaded = $state(false);
	context = $state<AiContext | null>(null);
	messages = $state<(AiMessage & { tools?: AiTool[] })[]>([]);
	allowData = $state(false);
	busy = $state(false);
	error = $state('');
	private requestId: string | null = null;
	private generation = 0;

	async load() {
		try {
			this.settings = await aiApi.getSettings();
			this.loaded = true;
		} catch (e) {
			this.error = errorMessage(e);
		}
	}

	setContext(context: AiContext | null) {
		if (
			this.context?.connectionId === context?.connectionId &&
			this.context?.database === context?.database
		)
			return;
		this.reset();
		this.context = context;
	}

	setAllowData(allow: boolean) {
		if (allow === this.allowData) return;
		this.reset();
		this.allowData = allow;
	}

	reset() {
		void this.cancel();
		this.generation++;
		this.messages = [];
		this.allowData = false;
		this.error = '';
	}

	async cancel() {
		const id = this.requestId;
		const generation = this.generation;
		this.requestId = null;
		this.busy = false;
		if (!id) return;
		if (this.messages.at(-1)?.role === 'user') this.messages = this.messages.slice(0, -1);
		try {
			await aiApi.cancel(id);
		} catch (e) {
			if (this.generation === generation && !this.requestId)
				this.error = `Could not cancel request: ${errorMessage(e)}`;
		}
	}

	async send(content: string) {
		if (!content.trim() || !this.context || this.busy || !this.loaded) return;
		const requestId = crypto.randomUUID();
		this.generation++;
		this.requestId = requestId;
		this.busy = true;
		this.error = '';
		this.messages = [...this.messages.slice(-38), { role: 'user', content: content.trim() }];
		try {
			const reply = await aiApi.chat({
				...this.context,
				requestId,
				messages: this.messages.map(({ role, content }) => ({ role, content })),
				allowData: this.allowData
			});
			if (this.requestId !== requestId) return;
			this.messages = [
				...this.messages,
				{ role: 'assistant', content: reply.text, tools: reply.tools }
			];
		} catch (e) {
			if (this.requestId === requestId) {
				this.messages = this.messages.slice(0, -1);
				this.error = errorMessage(e);
			}
		} finally {
			if (this.requestId === requestId) {
				this.requestId = null;
				this.busy = false;
			}
		}
	}
}

export const ai = new AiChat();
