import {
	aiApi,
	defaultAiConfig,
	type AiContext,
	type AiMessage,
	type AiActivity,
	type AiTool
} from '#lib/ai.js';
import { errorMessage } from '#lib/api.js';

/** Each level includes the ones before it. */
export type AiAccess = 'schema' | 'read' | 'write';

export class AiChat {
	settings = $state({ config: defaultAiConfig(), hasApiKey: false, hasBasicAuth: false });
	loaded = $state(false);
	context = $state<AiContext | null>(null);
	messages = $state<
		(AiMessage & { tools?: AiTool[]; activity?: AiActivity[]; durationMs?: number })[]
	>([]);
	allowData = $state(false);
	allowWrite = $state(false);
	busy = $state(false);
	error = $state('');
	activity = $state<AiActivity[]>([]);
	streamedText = $state('');
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

	get access(): AiAccess {
		return this.allowWrite ? 'write' : this.allowData ? 'read' : 'schema';
	}

	setAccess(access: AiAccess) {
		if (access === this.access) return;
		this.reset();
		this.allowData = access !== 'schema';
		this.allowWrite = access === 'write';
	}

	async decideWrite(approvalId: string, approved: boolean) {
		const requestId = this.requestId;
		const item = this.activity.find(
			(entry) => entry.type === 'tool' && entry.approval?.id === approvalId
		);
		if (
			!requestId ||
			!item ||
			item.type !== 'tool' ||
			!item.approval ||
			item.approval.decision !== undefined
		)
			return;
		item.approval.decision = approved;
		try {
			await aiApi.decideWrite(requestId, approvalId, approved);
		} catch (e) {
			if (this.requestId === requestId) {
				item.approval.decision = undefined;
				this.error = errorMessage(e);
			}
		}
	}

	reset() {
		void this.cancel();
		this.generation++;
		this.messages = [];
		this.allowData = false;
		this.allowWrite = false;
		this.error = '';
	}

	async cancel() {
		const id = this.requestId;
		const generation = this.generation;
		this.requestId = null;
		this.busy = false;
		this.activity = [];
		this.streamedText = '';
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
		const started = performance.now();
		this.generation++;
		this.requestId = requestId;
		this.busy = true;
		this.error = '';
		this.activity = [];
		this.streamedText = '';
		this.messages = [...this.messages.slice(-38), { role: 'user', content: content.trim() }];
		try {
			const reply = await aiApi.chat(
				{
					...this.context,
					requestId,
					messages: this.messages.map(({ role, content }) => ({ role, content })),
					allowData: this.allowData,
					allowWrite: this.allowWrite
				},
				(event) => {
					if (this.requestId !== requestId) return;
					const last = this.activity.at(-1);
					if (event.type === 'text') {
						this.streamedText += event.text;
					} else if (event.type === 'clearText') {
						this.streamedText = '';
					} else if (event.type === 'thinking') {
						if (last?.type === 'thinking') last.text += event.text;
						else this.activity.push({ type: 'thinking', text: event.text });
					} else if (event.type === 'toolStarted') {
						this.activity.push({ type: 'tool', name: event.name, summary: null });
					} else if (event.type === 'approval' && last?.type === 'tool') {
						last.approval = { id: event.id, sql: event.sql };
					} else if (event.type === 'toolFinished' && last?.type === 'tool') {
						last.summary = event.summary;
						if (event.detail) last.detail = event.detail;
					}
				}
			);
			if (this.requestId !== requestId) return;
			this.messages = [
				...this.messages,
				{
					role: 'assistant',
					content: reply.text,
					tools: reply.tools,
					activity: this.activity,
					durationMs: performance.now() - started
				}
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
				this.activity = [];
				this.streamedText = '';
			}
		}
	}
}

export const ai = new AiChat();
