import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { aiApi, defaultAiConfig, extractSqlBlocks } from './ai';

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
