import { describe, expect, it } from 'vitest';
import { chatContext, type ChatContext } from './chat-context.js';
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
