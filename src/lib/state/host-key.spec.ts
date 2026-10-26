import { describe, expect, it } from 'vitest';
import { HostKeyPrompt } from './host-key.svelte';

const question = (host: string, port = 22) => ({ host, port, fingerprint: `SHA256:${host}` });

describe('hostKeyPrompt', () => {
	it('keeps only the fields the dialog shows', () => {
		const prompt = new HostKeyPrompt();
		const error = { ...question('vps'), kind: 'unknownHostKey', key: 'ssh-ed25519 AAAA' };
		void prompt.ask(error);
		expect(prompt.pending).toEqual(question('vps'));
	});

	it('merges concurrent asks for the same host into one dialog', async () => {
		const prompt = new HostKeyPrompt();
		const first = prompt.ask(question('vps'));
		const second = prompt.ask(question('vps'));
		const third = prompt.ask(question('vps'));
		prompt.answer(true);
		await expect(Promise.all([first, second, third])).resolves.toEqual([true, true, true]);
		expect(prompt.pending).toBeNull();
	});

	it('declines the open ask when another host asks', async () => {
		const prompt = new HostKeyPrompt();
		const first = prompt.ask(question('a'));
		const otherPort = prompt.ask(question('a', 2222));
		await expect(first).resolves.toBe(false);
		expect(prompt.pending?.port).toBe(2222);
		prompt.answer(true);
		await expect(otherPort).resolves.toBe(true);
	});
});
