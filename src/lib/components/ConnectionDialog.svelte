<script lang="ts">
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import PlugZapIcon from '@lucide/svelte/icons/plug-zap';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import * as Tabs from '#lib/components/ui/tabs/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { toast } from 'svelte-sonner';
	import {
		api,
		errorMessage,
		type ConnectionConfig,
		type SecretStatus,
		type Secrets,
		type SshAuth,
		type SslMode
	} from '#lib/api.js';
	import { connections, newConnection } from '#lib/state/connections.svelte.js';

	interface Props {
		open: boolean;
		/** Connection to edit; null creates a new one. */
		editing: ConnectionConfig | null;
	}

	let { open = $bindable(), editing }: Props = $props();

	const COLORS = ['#ef4444', '#f97316', '#eab308', '#22c55e', '#3b82f6', '#a855f7'];
	const SSL_MODES: { value: SslMode; label: string }[] = [
		{ value: 'disable', label: 'Disable' },
		{ value: 'prefer', label: 'Prefer' },
		{ value: 'require', label: 'Require' },
		{ value: 'verify-full', label: 'Verify full' }
	];
	const AUTH_LABELS: Record<SshAuth['method'], string> = {
		agent: 'Automatic',
		key: 'Private key file',
		password: 'Password'
	};

	let form = $state<ConnectionConfig>(newConnection());
	let useSsh = $state(false);
	let ssh = $state({
		host: '',
		port: 22,
		user: '',
		method: 'agent' as SshAuth['method'],
		keyPath: ''
	});
	let secrets = $state<Secrets>({});
	let stored = $state<SecretStatus>({ password: false, sshPassword: false, sshPassphrase: false });
	let busy = $state<'test' | 'save' | null>(null);
	let problem = $state<string | null>(null);

	// Reset the form each time the dialog opens.
	$effect(() => {
		if (!open) return;
		const base = editing ? structuredClone($state.snapshot(editing)) : newConnection();
		form = base;
		useSsh = base.ssh !== null;
		ssh = {
			host: base.ssh?.host ?? '',
			port: base.ssh?.port ?? 22,
			user: base.ssh?.user ?? '',
			method: base.ssh?.auth.method ?? 'agent',
			keyPath: base.ssh?.auth.method === 'key' ? base.ssh.auth.keyPath : '~/.ssh/id_ed25519'
		};
		secrets = {};
		problem = null;
		stored = { password: false, sshPassword: false, sshPassphrase: false };
		if (editing) api.secretStatus(editing.id).then((s) => (stored = s));
	});

	function build(): ConnectionConfig {
		const auth: SshAuth =
			ssh.method === 'key'
				? { method: 'key', keyPath: ssh.keyPath.trim() }
				: { method: ssh.method };
		return {
			...form,
			name: form.name.trim() || form.host.trim(),
			host: form.host.trim(),
			user: form.user.trim(),
			database: form.database.trim() || 'postgres',
			port: Number(form.port) || 5432,
			ssh: useSsh
				? { host: ssh.host.trim(), port: Number(ssh.port) || 22, user: ssh.user.trim(), auth }
				: null
		};
	}

	async function test() {
		busy = 'test';
		problem = null;
		try {
			const version = await api.testConnection(build(), secrets);
			toast.success(`Connected: PostgreSQL ${version}`);
		} catch (e) {
			problem = errorMessage(e);
		} finally {
			busy = null;
		}
	}

	async function save(event: SubmitEvent) {
		event.preventDefault();
		busy = 'save';
		problem = null;
		try {
			await connections.save(build(), secrets);
			open = false;
		} catch (e) {
			problem = errorMessage(e);
		} finally {
			busy = null;
		}
	}

	const savedPlaceholder = (has: boolean) => (has ? '•••••••• saved in keychain' : '');
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-xl">
		<form onsubmit={save} class="grid gap-4">
			<Dialog.Header>
				<Dialog.Title>{editing ? 'Edit connection' : 'New connection'}</Dialog.Title>
				<Dialog.Description>
					Passwords are stored in the system keychain, never in a file.
				</Dialog.Description>
			</Dialog.Header>

			<Tabs.Root value="general">
				<Tabs.List class="w-full">
					<Tabs.Trigger value="general">General</Tabs.Trigger>
					<Tabs.Trigger value="ssh">
						SSH tunnel
						{#if useSsh}<span class="size-1.5 rounded-full bg-emerald-500"></span>{/if}
					</Tabs.Trigger>
				</Tabs.List>

				<Tabs.Content value="general" class="grid gap-3 pt-2">
					<div class="grid grid-cols-[1fr_auto] items-end gap-3">
						<div class="grid gap-1.5">
							<Label for="c-name">Name</Label>
							<Input id="c-name" bind:value={form.name} placeholder={form.host || 'My database'} />
						</div>
						<div class="flex h-8 items-center gap-1">
							<button
								type="button"
								class="size-5 rounded-full border-2 {form.color === null
									? 'border-foreground'
									: 'border-transparent'} bg-muted"
								title="No color"
								onclick={() => (form.color = null)}
							></button>
							{#each COLORS as color (color)}
								<button
									type="button"
									class="size-5 rounded-full border-2 {form.color === color
										? 'border-foreground'
										: 'border-transparent'}"
									style:background-color={color}
									title="Mark with color"
									onclick={() => (form.color = color)}
								></button>
							{/each}
						</div>
					</div>

					<div class="grid grid-cols-[1fr_6rem] gap-3">
						<div class="grid gap-1.5">
							<Label for="c-host">Host</Label>
							<Input
								id="c-host"
								bind:value={form.host}
								required
								autocapitalize="off"
								spellcheck={false}
							/>
						</div>
						<div class="grid gap-1.5">
							<Label for="c-port">Port</Label>
							<Input id="c-port" type="number" bind:value={form.port} min={1} max={65535} />
						</div>
					</div>
					{#if useSsh}
						<p class="-mt-1 text-xs text-muted-foreground">
							Seen from the SSH server. A Docker container name like
							<code class="font-mono">app-postgres-1</code> works too.
						</p>
					{/if}

					<div class="grid grid-cols-2 gap-3">
						<div class="grid gap-1.5">
							<Label for="c-user">User</Label>
							<Input
								id="c-user"
								bind:value={form.user}
								required
								autocapitalize="off"
								spellcheck={false}
							/>
						</div>
						<div class="grid gap-1.5">
							<Label for="c-pw">Password</Label>
							<Input
								id="c-pw"
								type="password"
								bind:value={secrets.password}
								placeholder={savedPlaceholder(stored.password)}
							/>
						</div>
					</div>

					<div class="grid grid-cols-2 gap-3">
						<div class="grid gap-1.5">
							<Label for="c-db">Database</Label>
							<Input id="c-db" bind:value={form.database} autocapitalize="off" spellcheck={false} />
						</div>
						<div class="grid gap-1.5">
							<Label>SSL</Label>
							<Select.Root type="single" bind:value={form.sslMode}>
								<Select.Trigger class="w-full">
									{SSL_MODES.find((m) => m.value === form.sslMode)?.label}
								</Select.Trigger>
								<Select.Content>
									{#each SSL_MODES as mode (mode.value)}
										<Select.Item value={mode.value} label={mode.label} />
									{/each}
								</Select.Content>
							</Select.Root>
						</div>
					</div>

					<Label class="flex items-center gap-2 font-normal">
						<Checkbox bind:checked={form.readOnly} />
						Read-only (every transaction is read-only)
					</Label>
				</Tabs.Content>

				<Tabs.Content value="ssh" class="grid gap-3 pt-2">
					<Label class="flex items-center gap-2 font-normal">
						<Checkbox bind:checked={useSsh} />
						Connect through an SSH server
					</Label>

					<fieldset disabled={!useSsh} class="grid gap-3 disabled:opacity-50">
						<div class="grid grid-cols-[1fr_6rem] gap-3">
							<div class="grid gap-1.5">
								<Label for="s-host">SSH host</Label>
								<Input
									id="s-host"
									bind:value={ssh.host}
									required={useSsh}
									placeholder="vps.example.com"
									autocapitalize="off"
									spellcheck={false}
								/>
							</div>
							<div class="grid gap-1.5">
								<Label for="s-port">Port</Label>
								<Input id="s-port" type="number" bind:value={ssh.port} min={1} max={65535} />
							</div>
						</div>

						<div class="grid grid-cols-2 gap-3">
							<div class="grid gap-1.5">
								<Label for="s-user">SSH user</Label>
								<Input
									id="s-user"
									bind:value={ssh.user}
									required={useSsh}
									autocapitalize="off"
									spellcheck={false}
								/>
							</div>
							<div class="grid gap-1.5">
								<Label>Authentication</Label>
								<Select.Root type="single" bind:value={ssh.method}>
									<Select.Trigger class="w-full">{AUTH_LABELS[ssh.method]}</Select.Trigger>
									<Select.Content>
										{#each Object.entries(AUTH_LABELS) as [value, label] (value)}
											<Select.Item {value} {label} />
										{/each}
									</Select.Content>
								</Select.Root>
							</div>
						</div>

						{#if ssh.method === 'key'}
							<div class="grid grid-cols-2 gap-3">
								<div class="grid gap-1.5">
									<Label for="s-key">Private key file</Label>
									<Input
										id="s-key"
										bind:value={ssh.keyPath}
										required={useSsh}
										class="font-mono text-xs"
										spellcheck={false}
									/>
								</div>
								<div class="grid gap-1.5">
									<Label for="s-pass">Passphrase</Label>
									<Input
										id="s-pass"
										type="password"
										bind:value={secrets.sshPassphrase}
										placeholder={savedPlaceholder(stored.sshPassphrase) || 'if the key has one'}
									/>
								</div>
							</div>
						{:else if ssh.method === 'password'}
							<div class="grid gap-1.5">
								<Label for="s-pw">SSH password</Label>
								<Input
									id="s-pw"
									type="password"
									bind:value={secrets.sshPassword}
									placeholder={savedPlaceholder(stored.sshPassword)}
								/>
							</div>
						{:else}
							<div class="grid gap-1.5">
								<p class="text-xs text-muted-foreground">
									Like <code class="font-mono">ssh</code> in the terminal: keys from the SSH agent,
									then <code class="font-mono">~/.ssh/id_ed25519</code>,
									<code class="font-mono">id_ecdsa</code>, <code class="font-mono">id_rsa</code>.
								</p>
								<Label for="s-auto-pass">Key passphrase</Label>
								<Input
									id="s-auto-pass"
									type="password"
									bind:value={secrets.sshPassphrase}
									placeholder={savedPlaceholder(stored.sshPassphrase) || 'only if your key has one'}
								/>
							</div>
						{/if}
						<p class="text-xs text-muted-foreground">
							The server's host key is checked against <code class="font-mono"
								>~/.ssh/known_hosts</code
							>. Unknown keys are shown for confirmation first.
						</p>
					</fieldset>
				</Tabs.Content>
			</Tabs.Root>

			{#if problem}
				<p
					class="rounded-md border border-destructive/40 bg-destructive/5 p-2 text-sm break-words text-destructive select-text"
				>
					{problem}
				</p>
			{/if}

			<Dialog.Footer class="gap-2">
				<Button type="button" variant="outline" disabled={busy !== null} onclick={test}>
					{#if busy === 'test'}<LoaderIcon class="animate-spin" />{:else}<PlugZapIcon />{/if}
					Test
				</Button>
				<Button type="submit" disabled={busy !== null}>
					{#if busy === 'save'}<LoaderIcon class="animate-spin" />{/if}
					Save
				</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
