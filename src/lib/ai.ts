import { Channel, invoke } from '@tauri-apps/api/core';

export type AiProvider = 'ollama' | 'openaiResponses' | 'openaiCompatible';
export interface BasicAuth {
	username: string;
	password: string;
}
export interface AiConfig {
	provider: AiProvider;
	baseUrl: string;
	model: string;
	authMode: 'none' | 'bearer' | 'basic';
}
export interface AiSettings {
	config: AiConfig;
	hasApiKey: boolean;
	hasBasicAuth: boolean;
}
export interface AiContext {
	connectionId: string;
	database: string;
}
export interface AiMessage {
	role: 'user' | 'assistant';
	content: string;
}
export interface AiTool {
	name: string;
	summary: string;
}
export type AiEvent =
	| { type: 'thinking'; text: string }
	| { type: 'text'; text: string }
	| { type: 'clearText' }
	| { type: 'toolStarted'; name: string }
	| { type: 'toolFinished'; summary: string; detail?: string }
	| { type: 'approval'; id: string; sql: string };
export type AiActivity =
	| { type: 'thinking'; text: string }
	| {
			type: 'tool';
			name: string;
			summary: string | null;
			/** UI-only explanation of a failure or a short preview of the tool output. */
			detail?: string;
			approval?: { id: string; sql: string; decision?: boolean };
	  };
export function completedActivityCount(items: AiActivity[]): number {
	return items.filter((item) => item.type === 'tool' && item.summary !== null).length;
}

export function currentActivityLabel(items: AiActivity[]): string {
	const current = items.at(-1);
	if (!current || current.type === 'thinking' || current.summary !== null) return 'Thinking…';
	if (current.approval) {
		if (current.approval.decision === true) return 'Executing approved statement…';
		if (current.approval.decision === false) return 'Processing decision…';
		return 'Waiting for approval…';
	}
	switch (current.name) {
		case 'list_schemas':
			return 'Inspecting schemas…';
		case 'list_tables':
			return 'Inspecting tables…';
		case 'describe_table':
			return 'Inspecting table…';
		case 'sample_rows':
			return 'Reading sample rows…';
		case 'query_rows':
			return 'Running query…';
		case 'propose_write':
			return 'Preparing change…';
		default:
			return `Running ${current.name.replaceAll('_', ' ')}…`;
	}
}

/**
 * Write approvals render outside the collapsible history so the user always sees what they
 * approved or rejected, not only the approval that is still pending.
 */
export function splitApprovals(items: AiActivity[]): {
	history: AiActivity[];
	approvals: AiActivity[];
} {
	const history: AiActivity[] = [];
	const approvals: AiActivity[] = [];
	for (const item of items) {
		(item.type === 'tool' && item.approval ? approvals : history).push(item);
	}
	return { history, approvals };
}

export type ToolState = 'running' | 'done' | 'blocked' | 'failed';
export interface ToolOutcome {
	state: ToolState;
	/** Headline without the redundant "Completed" prefix; empty when there is nothing to add. */
	label: string;
	/** Lines after the headline, e.g. the executed SQL and its parameters. */
	details: string;
}

export function toolOutcome(summary: string | null): ToolOutcome {
	if (summary === null) return { state: 'running', label: '', details: '' };
	const [headline, ...rest] = summary.split('\n');
	const details = rest.join('\n').trim();
	if (headline.startsWith('Completed')) {
		return { state: 'done', label: headline.replace(/^Completed(\s*·\s*)?/, ''), details };
	}
	const state = /^(Blocked|Rejected)\b/.test(headline) ? 'blocked' : 'failed';
	return { state, label: headline, details };
}

export interface ApprovalOutcome {
	tone: 'ok' | 'muted' | 'bad';
	title: string;
	detail: string;
}

export function approvalOutcome(summary: string): ApprovalOutcome {
	const outcome = toolOutcome(summary);
	if (outcome.state === 'done')
		return { tone: 'ok', title: 'Change approved', detail: outcome.label };
	const [status, ...rest] = outcome.label.split(' · ');
	return {
		tone: outcome.state === 'blocked' ? 'muted' : 'bad',
		title: `Change ${status.toLowerCase()}`,
		detail: rest.join(' · ')
	};
}

/** One-line plain-text preview of a reasoning block. */
export function thinkingPreview(text: string): string {
	return text
		.replace(/\*\*|__|`/g, '')
		.replace(/^\s*#+\s*/gm, '')
		.replace(/\s+/g, ' ')
		.trim();
}
export interface AiReply {
	text: string;
	tools: AiTool[];
}
export interface AiRequest extends AiContext {
	requestId: string;
	messages: AiMessage[];
	allowData: boolean;
	allowWrite: boolean;
}
	confirmReads: boolean;

export function defaultAiConfig(provider: AiProvider = 'ollama'): AiConfig {
	return {
		provider,
		baseUrl: provider === 'ollama' ? 'http://localhost:11434' : 'https://api.openai.com/v1',
		model: '',
		authMode: 'bearer'
	};
}

export const aiApi = {
	getSettings: () => invoke<AiSettings>('ai_get_settings'),
	saveSettings: (config: AiConfig, apiKey?: string, basicAuth?: BasicAuth) =>
		invoke<AiSettings>('ai_save_settings', {
			config,
			...(apiKey === undefined ? {} : { apiKey }),
			...(basicAuth === undefined ? {} : { basicAuth })
		}),
	models: (config: AiConfig, apiKey?: string, basicAuth?: BasicAuth) =>
		invoke<string[]>('ai_ollama_models', { config, apiKey, basicAuth }),
	chat: (request: AiRequest, onEvent: (event: AiEvent) => void) => {
		const channel = new Channel<AiEvent>();
		channel.onmessage = onEvent;
		return invoke<AiReply>('ai_chat', { request, onEvent: channel });
	},
	decideWrite: (requestId: string, approvalId: string, approved: boolean) =>
		invoke<void>('ai_decide_write', { requestId, approvalId, approved }),
	cancel: (requestId: string) => invoke<void>('ai_cancel', { requestId })
};

/** Only explicitly labelled, closed SQL fences can become editor actions. */
export function extractSqlBlocks(text: string): string[] {
	const blocks: string[] = [];
	const fence = /^ {0,3}(`{3,}|~{3,})([^\n]*)\r?\n/gm;
	let match: RegExpExecArray | null;
	while ((match = fence.exec(text))) {
		const marker = match[1];
		const closing = new RegExp(`^ {0,3}${marker[0]}{${marker.length},}[ \\t]*\\r?$`, 'gm');
		closing.lastIndex = fence.lastIndex;
		const end = closing.exec(text);
		if (!end) break;
		if (/^(sql|postgresql|postgres)$/i.test(match[2].trim())) {
			const sql = text.slice(fence.lastIndex, end.index).trim();
			if (sql) blocks.push(sql);
		}
		fence.lastIndex = closing.lastIndex;
	}
	return blocks;
}
