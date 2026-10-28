<script lang="ts">
	import UsersIcon from '@lucide/svelte/icons/users';
	import UserIcon from '@lucide/svelte/icons/user';
	import ShieldUserIcon from '@lucide/svelte/icons/shield-user';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import * as ContextMenu from '#lib/components/ui/context-menu/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import { api, errorMessage } from '#lib/api.js';
	import { Resource } from '#lib/resource.svelte.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { roleEvents } from '#lib/state/roles.svelte.js';
	import { tabs } from '#lib/state/tabs.svelte.js';
	import TreeRow from './TreeRow.svelte';
	import { groupRoles, roleTitle } from './roles.js';

	let {
		connectionId,
		readOnly,
		depth
	}: { connectionId: string; readOnly: boolean; depth: number } = $props();
	let expanded = $state(false);
	const roles = new Resource(() => api.listRoles(connectionId));
	const error = $derived(roles.failed ? errorMessage(roles.error) : null);
	let dialog = $state(false);
	let busy = $state(false);
	let name = $state('');
	let password = $state('');
	let login = $state(true);
	let createDb = $state(false);
	let createRole = $state(false);
	let problem = $state<string | null>(null);

	let groups = $derived(groupRoles(roles.data ?? []));

	// Reload after a role tab changed or dropped a role.
	let seenRevision = roleEvents.revision;
	$effect(() => {
		const revision = roleEvents.revision;
		if (revision === seenRevision) return;
		seenRevision = revision;
		if (expanded) void load();
	});

	function open(role: string) {
		tabs.openRole(connectionId, connections.get(connectionId)?.database ?? '', role);
	}
	function isOpen(role: string): boolean {
		const active = tabs.active;
		return active?.kind === 'role' && active.connectionId === connectionId && active.role === role;
	}

	function load() {
		void roles.load();
	}
	function toggle() {
		expanded = !expanded;
		if (expanded) void load();
	}
	function openCreate() {
		name = '';
		password = '';
		login = true;
		createDb = false;
		createRole = false;
		problem = null;
		dialog = true;
	}
	async function submit(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		problem = null;
		try {
			await api.createRole(connectionId, name, password || null, login, createDb, createRole);
			password = '';
			dialog = false;
			expanded = true;
			roleEvents.changed();
			open(name);
		} catch (e) {
			problem = errorMessage(e);
		} finally {
			busy = false;
		}
	}
</script>

<ContextMenu.Root>
	<ContextMenu.Trigger>
		{#snippet child({ props })}
			<TreeRow
				{...props}
				{depth}
				{expanded}
				loading={roles.loading}
				label="Users & roles"
				onclick={toggle}
			>
				{#snippet icon()}<UsersIcon class="size-3.5" />{/snippet}
			</TreeRow>
		{/snippet}
	</ContextMenu.Trigger>
	<ContextMenu.Content class="w-52">
		<ContextMenu.Item disabled={readOnly} onSelect={openCreate}>New user or role…</ContextMenu.Item>
		<ContextMenu.Item onSelect={load}>Refresh</ContextMenu.Item>
	</ContextMenu.Content>
</ContextMenu.Root>

{#if expanded}
	{#if error}
		<p
			class="py-1 pr-2 text-xs break-words text-destructive"
			style:padding-left="{(depth + 1) * 10 + 22}px"
			role="alert"
		>
			{error}
		</p>
	{:else if !roles.loading && roles.data?.length === 0}
		<p
			class="py-1 text-[13px] text-muted-foreground"
			style:padding-left="{(depth + 1) * 10 + 22}px"
		>
			No roles
		</p>
	{/if}
	{#each groups as group (group.label)}
		{#if groups.length > 1}
			<p
				class="pt-1.5 pb-0.5 text-[11px] text-muted-foreground"
				style:padding-left="{(depth + 1) * 10 + 22}px"
			>
				{group.label}
			</p>
		{/if}
		{#each group.roles as role (role.name)}
			<TreeRow
				depth={depth + 1}
				label={role.name}
				title={roleTitle(role)}
				class={isOpen(role.name) ? 'bg-hover text-foreground' : ''}
				onclick={() => open(role.name)}
			>
				{#snippet icon()}
					{#if role.superuser}
						<ShieldUserIcon class="size-3.5" />
					{:else if role.canLogin}
						<UserIcon class="size-3.5" />
					{:else}
						<UsersIcon class="size-3.5" />
					{/if}
				{/snippet}
				{#snippet trailing()}
					{#if role.superuser}
						<span class="text-[11px] text-muted-foreground">superuser</span>
					{/if}
				{/snippet}
			</TreeRow>
		{/each}
	{/each}
	{#if !readOnly}
		<TreeRow
			depth={depth + 1}
			label="New user or role…"
			class="text-muted-foreground"
			onclick={openCreate}
		>
			{#snippet icon()}<PlusIcon class="size-3.5" />{/snippet}
		</TreeRow>
	{/if}
{/if}

<Dialog.Root
	open={dialog}
	onOpenChange={(open) => {
		if (busy) return;
		dialog = open;
		if (!open) password = '';
	}}
>
	<Dialog.Content class="sm:max-w-md">
		<form class="grid gap-4" onsubmit={submit}>
			<Dialog.Header
				><Dialog.Title>New user or role</Dialog.Title><Dialog.Description
					>Roles are shared by all databases on this PostgreSQL server.</Dialog.Description
				></Dialog.Header
			>
			<p class="text-xs text-muted-foreground">
				Database and table privileges must be granted separately.
			</p>
			<div class="grid gap-1.5">
				<Label for="role-name">Name</Label><Input
					id="role-name"
					bind:value={name}
					required
					autocomplete="off"
				/>
			</div>
			<div class="grid gap-1.5">
				<Label for="role-password">Password (optional)</Label><Input
					id="role-password"
					type="password"
					bind:value={password}
					autocomplete="new-password"
				/>
			</div>
			<label class="flex items-center gap-2"><Checkbox bind:checked={login} /> Can log in</label>
			<label class="flex items-center gap-2"
				><Checkbox bind:checked={createDb} /> Can create databases</label
			>
			<label class="flex items-center gap-2"
				><Checkbox bind:checked={createRole} /> Can create roles</label
			>
			{#if problem}<p class="text-xs text-destructive" role="alert">{problem}</p>{/if}
			<Dialog.Footer
				><Button
					type="button"
					variant="outline"
					disabled={busy}
					onclick={() => {
						dialog = false;
						password = '';
					}}>Cancel</Button
				><Button type="submit" disabled={busy || !name.trim()}>Create</Button></Dialog.Footer
			>
		</form>
	</Dialog.Content>
</Dialog.Root>
