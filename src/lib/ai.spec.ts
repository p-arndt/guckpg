import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
	aiApi,
	defaultAiConfig,
	extractSqlBlocks,
	splitApprovals,
	currentActivityLabel,
	completedActivityCount,
	toolOutcome,
	approvalOutcome,
	thinkingPreview,
	modelCredentials,
	settingsPayload,
	withTimeout,
	type AiCredentialsDraft
} from './ai';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('SQL editor suggestions', () => {
	it('extracts multiple SQL fences and leaves other languages untouched', () => {
		expect(
			extractSqlBlocks(
				'```sql\nSELECT 1;\n```\n```js\nalert(1)\n```\n~~~PostgreSQL\nSELECT 2;\n~~~'
			)
		).toEqual(['SELECT 1;', 'SELECT 2;']);
	});
	it('rejects unfinished, empty, unlabelled and nested non-SQL fences', () => {
		expect(extractSqlBlocks('```\n```sql\nSELECT 1;\n```\n')).toEqual([]);
		expect(extractSqlBlocks('```sql\nSELECT 1;')).toEqual([]);
		expect(extractSqlBlocks('```sql\n\n```')).toEqual([]);
	});
	it('supports CRLF and requires matching fence length', () => {
		expect(extractSqlBlocks('````sql\r\nSELECT 1;\r\n```\r\nSELECT 2;\r\n````')).toEqual([
			'SELECT 1;\r\n```\r\nSELECT 2;'
		]);
	});
});

describe('provider settings', () => {
	it('uses local Ollama by default and the v1 API root for OpenAI', () => {
		expect(defaultAiConfig()).toEqual({
			provider: 'ollama',
			baseUrl: 'http://localhost:11434',
			model: '',
			authMode: 'bearer'
		});
		expect(defaultAiConfig('openaiResponses').baseUrl).toBe('https://api.openai.com/v1');
		expect(defaultAiConfig('openaiCompatible').provider).toBe('openaiCompatible');
	});
	it('loads Ollama models with saved credentials by default and accepts unsaved credentials', async () => {
		const config = { ...defaultAiConfig(), authMode: 'basic' as const };
		await aiApi.models(config);
		expect(invoke).toHaveBeenLastCalledWith('ai_ollama_models', {
			config,
			apiKey: undefined,
			basicAuth: undefined
		});
		const basicAuth = { username: 'user', password: 'pass' };
		await aiApi.models(config, undefined, basicAuth);
		expect(invoke).toHaveBeenLastCalledWith('ai_ollama_models', {
			config,
			apiKey: undefined,
			basicAuth
		});
	});
	it('distinguishes preserving and deleting a saved key', async () => {
		await aiApi.saveSettings(defaultAiConfig());
		expect(invoke).toHaveBeenLastCalledWith('ai_save_settings', { config: defaultAiConfig() });
		await aiApi.saveSettings(defaultAiConfig(), '');
		expect(invoke).toHaveBeenLastCalledWith('ai_save_settings', {
			config: defaultAiConfig(),
			apiKey: ''
		});
	});

	it('submits Basic Auth separately from non-secret config and supports deletion', async () => {
		const config = { ...defaultAiConfig(), authMode: 'basic' as const };
		const credentials = { username: 'proxy-user', password: 'test-password' };
		await aiApi.saveSettings(config, undefined, credentials);
		expect(invoke).toHaveBeenLastCalledWith('ai_save_settings', { config, basicAuth: credentials });
		expect(JSON.stringify(config)).not.toContain('test-password');
		await aiApi.saveSettings(config);
		expect(invoke).toHaveBeenLastCalledWith('ai_save_settings', { config });
		await aiApi.saveSettings(config, undefined, { username: '', password: '' });
		expect(invoke).toHaveBeenLastCalledWith('ai_save_settings', {
			config,
			basicAuth: { username: '', password: '' }
		});
	});
});

describe('activity grouping', () => {
	it('keeps activity order and keeps pending and resolved approvals outside the group', () => {
		const thinking = { type: 'thinking' as const, text: 'thinking' };
		const read = { type: 'tool' as const, name: 'query_rows', summary: 'Completed' };
		const pending = {
			type: 'tool' as const,
			name: 'propose_write',
			summary: null,
			approval: { id: 'one', sql: 'DELETE FROM t' }
		};
		expect(splitApprovals([thinking, read, pending])).toEqual({
			history: [thinking, read],
			approvals: [pending]
		});
		const complete = { ...pending, summary: 'Completed · 1 rows affected' };
		const later = { type: 'tool' as const, name: 'list_tables', summary: 'Completed' };
		expect(splitApprovals([thinking, read, complete, later])).toEqual({
			history: [thinking, read, later],
			approvals: [complete]
		});
	});
});

describe('tool outcome', () => {
	it('reports running tools and strips the completed prefix', () => {
		expect(toolOutcome(null)).toEqual({ state: 'running', label: '', details: '' });
		expect(toolOutcome('Completed')).toEqual({ state: 'done', label: '', details: '' });
		expect(toolOutcome('Completed · 3 rows affected').label).toBe('3 rows affected');
	});
	it('splits multi-line summaries into a headline and details', () => {
		expect(toolOutcome('Completed\nSELECT 1\nParameters: []')).toEqual({
			state: 'done',
			label: '',
			details: 'SELECT 1\nParameters: []'
		});
	});
	it('separates blocked and rejected calls from failures', () => {
		expect(toolOutcome('Blocked').state).toBe('blocked');
		expect(toolOutcome('Rejected').state).toBe('blocked');
		expect(toolOutcome('Failed').state).toBe('failed');
		expect(toolOutcome('Timed out').state).toBe('failed');
		expect(toolOutcome('Timed out').label).toBe('Timed out');
	});
});

describe('approval outcome', () => {
	it('describes approved, rejected and failed changes', () => {
		expect(approvalOutcome('Completed · 1 rows affected')).toEqual({
			tone: 'ok',
			title: 'Change approved',
			detail: '1 rows affected'
		});
		expect(approvalOutcome('Rejected')).toEqual({
			tone: 'muted',
			title: 'Change rejected',
			detail: ''
		});
		expect(approvalOutcome('Blocked').tone).toBe('muted');
		expect(approvalOutcome('Failed')).toEqual({ tone: 'bad', title: 'Change failed', detail: '' });
		expect(approvalOutcome('Timed out · verify database before retrying')).toEqual({
			tone: 'bad',
			title: 'Change timed out',
			detail: 'verify database before retrying'
		});
	});
});

describe('thinking preview', () => {
	it('flattens markdown and whitespace into one line', () => {
		expect(thinkingPreview('## Plan\n\n**Check** the `users`\n  table_name first.')).toBe(
			'Plan Check the users table_name first.'
		);
		expect(thinkingPreview('  \n ')).toBe('');
	});
});

describe('current activity label', () => {
	it('shows the current step and returns to thinking after a tool finishes', () => {
		expect(currentActivityLabel([])).toBe('Thinking…');
		expect(currentActivityLabel([{ type: 'tool', name: 'list_tables', summary: null }])).toBe(
			'Inspecting tables…'
		);
		expect(currentActivityLabel([{ type: 'tool', name: 'query_rows', summary: null }])).toBe(
			'Running query…'
		);
		expect(currentActivityLabel([{ type: 'tool', name: 'query_rows', summary: 'Completed' }])).toBe(
			'Thinking…'
		);
		expect(currentActivityLabel([{ type: 'thinking', text: 'reasoning' }])).toBe('Thinking…');
	});
	it('distinguishes waiting for approval from executing an approved write', () => {
		const item = {
			type: 'tool' as const,
			name: 'propose_write',
			summary: null,
			approval: { id: 'one', sql: 'UPDATE t SET x=1' }
		};
		expect(currentActivityLabel([item])).toBe('Waiting for approval…');
		expect(
			currentActivityLabel([{ ...item, approval: { ...item.approval, decision: true } }])
		).toBe('Executing approved statement…');
		expect(
			currentActivityLabel([{ ...item, approval: { ...item.approval, decision: false } }])
		).toBe('Processing decision…');
	});
});

describe('completed activity count', () => {
	it('counts finished tool calls, including failures, without counting thinking or pending approvals', () => {
		expect(
			completedActivityCount([
				{ type: 'thinking', text: 'thinking' },
				{ type: 'tool', name: 'query_rows', summary: 'Completed' },
				{ type: 'tool', name: 'query_rows', summary: 'Failed' },
				{
					type: 'tool',
					name: 'propose_write',
					summary: null,
					approval: { id: 'one', sql: 'DELETE FROM t' }
				}
			])
		).toBe(2);
	});
});

describe('settings payload', () => {
	const blank: AiCredentialsDraft = {
		apiKey: '',
		clearKey: false,
		basicUsername: '',
		basicPassword: '',
		clearBasic: false
	};
	const bearer = { ...defaultAiConfig('openaiResponses'), baseUrl: ' https://x/v1 ', model: ' m ' };
	const basic = { ...bearer, authMode: 'basic' as const };

	it('trims the endpoint and model', () => {
		expect(settingsPayload(bearer, blank, true).config).toMatchObject({
			baseUrl: 'https://x/v1',
			model: 'm'
		});
	});

	it('keeps, replaces or deletes the API key', () => {
		expect(settingsPayload(bearer, blank, true).apiKey).toBeUndefined();
		expect(settingsPayload(bearer, { ...blank, apiKey: 'k' }, true).apiKey).toBe('k');
		expect(settingsPayload(bearer, { ...blank, apiKey: 'k', clearKey: true }, true).apiKey).toBe(
			''
		);
		// A delete ticked for another endpoint must not wipe the key of the new one.
		expect(settingsPayload(bearer, { ...blank, apiKey: 'k', clearKey: true }, false).apiKey).toBe(
			'k'
		);
		expect(settingsPayload(basic, { ...blank, apiKey: 'k' }, true).apiKey).toBeUndefined();
	});

	it('keeps, replaces or deletes Basic Auth', () => {
		expect(settingsPayload(basic, blank, true).basicAuth).toBeUndefined();
		expect(settingsPayload(basic, { ...blank, basicUsername: 'u' }, true).basicAuth).toEqual({
			username: 'u',
			password: ''
		});
		expect(
			settingsPayload(basic, { ...blank, basicUsername: 'u', clearBasic: true }, true).basicAuth
		).toEqual({ username: '', password: '' });
		expect(settingsPayload(basic, { ...blank, clearBasic: true }, false).basicAuth).toBeUndefined();
		expect(
			settingsPayload(bearer, { ...blank, basicUsername: 'u' }, true).basicAuth
		).toBeUndefined();
	});

	it('sends only the credentials of the chosen auth mode when listing models', () => {
		const draft = { ...blank, apiKey: 'k', basicPassword: 'p' };
		expect(modelCredentials(bearer, draft)).toEqual({ apiKey: 'k', basicAuth: undefined });
		expect(modelCredentials(basic, draft)).toEqual({
			apiKey: undefined,
			basicAuth: { username: '', password: 'p' }
		});
	});
});

describe('withTimeout', () => {
	it('passes the result through', async () => {
		await expect(withTimeout(Promise.resolve(1), 1000, 'slow')).resolves.toBe(1);
	});

	it('rejects with the message when too slow', async () => {
		vi.useFakeTimers();
		try {
			const result = withTimeout(new Promise(() => {}), 1000, 'slow');
			vi.advanceTimersByTime(1000);
			await expect(result).rejects.toThrow('slow');
		} finally {
			vi.useRealTimers();
		}
	});
});
