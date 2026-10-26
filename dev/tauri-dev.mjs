import { spawnSync } from 'node:child_process';

const env = { ...process.env };
if (process.platform === 'darwin') {
	const identity = spawnSync('security', ['find-identity', '-v', '-p', 'codesigning'], {
		encoding: 'utf8'
	});
	if (
		identity.status === 0 &&
		identity.stdout.includes('"guckpg Local Development Code Signing"')
	) {
		const target = process.arch === 'arm64' ? 'AARCH64' : 'X86_64';
		env[`CARGO_TARGET_${target}_APPLE_DARWIN_RUNNER`] = 'sh ../dev/sign-and-run.sh';
	}
}

const result = spawnSync('pnpm', ['exec', 'tauri', 'dev', ...process.argv.slice(2)], {
	env,
	stdio: 'inherit'
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
