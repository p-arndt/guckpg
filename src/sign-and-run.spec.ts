import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { afterEach, describe, expect, it } from 'vitest';

let dir: string;
afterEach(() => rmSync(dir, { recursive: true, force: true }));

describe('development signing runner', () => {
	it.each([0, 1])('starts the app only when signing exits with %i', (status) => {
		dir = mkdtempSync(join(tmpdir(), 'guckpg-sign-test-'));
		writeFileSync(join(dir, 'codesign'), `#!/bin/sh\nexit ${status}\n`, { mode: 0o755 });
		const app = join(dir, 'app');
		const output = join(dir, 'output');
		writeFileSync(app, `#!/bin/sh\nprintf '%s' "$1" > "$OUTPUT"\n`, { mode: 0o755 });
		const result = spawnSync('sh', [resolve('dev/sign-and-run.sh'), app, 'argument'], {
			env: { ...process.env, PATH: `${dir}:${process.env.PATH}`, OUTPUT: output }
		});
		expect(result.status).toBe(status);
		if (status === 0) expect(readFileSync(output, 'utf8')).toBe('argument');
		else expect(existsSync(output)).toBe(false);
	});
});
