import { describe, expect, it } from 'vitest';
import { activeChatContext, chatContext, contextKey, type ChatContext } from './chat-context.js';
import type { ConnectionConfig } from './api.js';

const base: ChatContext = {
	connectionId: 'one',
	database: 'first',
	name: 'One',
	user: 'alice',
	readOnly: false
};
const other = {
	id: 'two',
	name: 'Two',
	user: 'bob',
	readOnly: true
} as ConnectionConfig;

describe('chat context selection', () => {
	it('uses the selected connection and database, including its access settings', () => {
		expect(
			chatContext(base, { connectionId: 'two', database: 'second' }, [other], { two: 'connected' })
		).toEqual({
			connectionId: 'two',
			database: 'second',
			name: 'Two',
			user: 'bob',
			readOnly: true
		});
	});

	it('falls back to the active context when the selected connection is unavailable', () => {
		const selected = { connectionId: 'two', database: 'second' };
		expect(chatContext(base, selected, [other], { two: 'idle' })).toEqual(base);
		expect(chatContext(base, selected, [], { two: 'connected' })).toEqual(base);
		expect(chatContext(null, selected, [other], { two: 'connected' })).toBeNull();
	});
});

describe('active chat context', () => {
	const one = { id: 'one', name: 'One', user: 'alice', database: 'home1', readOnly: false };
	const two = { id: 'two', name: 'Two', user: 'bob', database: 'home2', readOnly: true };
	const list = [one, two] as ConnectionConfig[];
	const both = { one: 'connected', two: 'connected' };

	it("follows the active tab's connection and database", () => {
		expect(activeChatContext({ connectionId: 'two', database: 'sales' }, list, both, {})).toEqual({
			connectionId: 'two',
			database: 'sales',
			name: 'Two',
			user: 'bob',
			readOnly: true
		});
	});

	it('falls back to the first connected connection and its current database', () => {
		const tab = { connectionId: 'one', database: 'sales' };
		expect(
			activeChatContext(tab, list, { two: 'connected' }, { two: { database: 'live' } })
		).toMatchObject({ connectionId: 'two', database: 'live' });
		expect(activeChatContext(undefined, list, { two: 'connected' }, {})).toMatchObject({
			connectionId: 'two',
			database: 'home2'
		});
	});

	it('is empty without a connected connection', () => {
		const tab = { connectionId: 'one', database: 'x' };
		expect(activeChatContext(tab, list, { one: 'idle' }, {})).toBeNull();
	});
});

describe('context key', () => {
	it('ignores display fields', () => {
		const renamed: ChatContext = { ...base, name: 'Renamed' };
		expect(contextKey(renamed)).toBe(contextKey(base));
		expect(contextKey({ ...base, database: 'other' })).not.toBe(contextKey(base));
		expect(contextKey(null)).toBe('');
	});
});
