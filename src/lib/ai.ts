import { invoke } from '@tauri-apps/api/core';

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
export interface AiReply {
	text: string;
	tools: AiTool[];
}
export interface AiRequest extends AiContext {
	requestId: string;
	messages: AiMessage[];
	allowData: boolean;
}

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
	chat: (request: AiRequest) => invoke<AiReply>('ai_chat', { request }),
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
