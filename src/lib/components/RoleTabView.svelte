<script lang="ts">
	import { onMount } from 'svelte';
	import LoaderIcon from '@lucide/svelte/icons/loader-circle';
	import LockIcon from '@lucide/svelte/icons/lock';
	import RefreshIcon from '@lucide/svelte/icons/refresh-cw';
	import ShieldUserIcon from '@lucide/svelte/icons/shield-user';
	import TrashIcon from '@lucide/svelte/icons/trash';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
	import UserIcon from '@lucide/svelte/icons/user';
	import UsersIcon from '@lucide/svelte/icons/users';
	import XIcon from '@lucide/svelte/icons/x';
	import {
		api,
		errorMessage,
		type ObjectPrivileges,
		type RoleDetails,
		type RolePrivileges
	} from '#lib/api.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Checkbox } from '#lib/components/ui/checkbox/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import { highlightSql } from '#lib/highlight.js';
	import {
		DATABASE_PRIVILEGES,
		SCHEMA_PRIVILEGES,
		TABLE_PRIVILEGES,
		draftOf,
		filterObjects,
		grantsOf,
		planChanges,
		targetKey,
		targetOf,
		toggleColumn,
		toggleGrant,
		type Grants,
		type RoleDraft
	} from '#lib/role-editor.js';
	import { confirmation, confirmDiscard } from '#lib/state/confirm.svelte.js';
	import { connections } from '#lib/state/connections.svelte.js';
	import { roleEvents } from '#lib/state/roles.svelte.js';
	import { tabs, type RoleTab } from '#lib/state/tabs.svelte.js';

	let { tab }: { tab: RoleTab } = $props();

	type Section = 'general' | 'membership' | 'privileges';
	const SECTIONS: { id: Section; label: string }[] = [
		{ id: 'general', label: 'General' },
		{ id: 'membership', label: 'Membership' },
		{ id: 'privileges', label: 'Privileges' }
	];
	const CAPABILITIES: { key: keyof RoleDraft; label: string; hint: string }[] = [
		{
			key: 'superuser',
			label: 'Superuser',
			hint: 'Bypasses every permission check. Only for administrators.'
		},
		{ key: 'createDb', label: 'Create databases', hint: 'Can create new databases.' },
		{ key: 'createRole', label: 'Create roles', hint: 'Can create, change and drop other roles.' },
		{
			key: 'inherit',
			label: 'Inherit privileges',
			hint: 'Uses the privileges of its member roles without SET ROLE.'
		},
		{
			key: 'replication',
			label: 'Replication',
			hint: 'Can stream replication and manage replication slots.'
		},
		{ key: 'bypassRls', label: 'Bypass row-level security', hint: 'Ignores every RLS policy.' }
	];
	const LABELS: Record<string, string> = {
		TEMPORARY: 'Temp',
		REFERENCES: 'References'
	};
	const label = (privilege: string) =>
		LABELS[privilege] ?? privilege.charAt(0) + privilege.slice(1).toLowerCase();

	let section = $state<Section>('general');
	let loading = $state(true);
	let error = $state<string | null>(null);
	let details = $state.raw<RoleDetails | null>(null);
	let draft = $state<RoleDraft | null>(null);
	let databaseGrants = $state<Grants>({});
	let originalDatabaseGrants = $state.raw<Grants>({});

	let privileges = $state.raw<RolePrivileges | null>(null);
	let privilegesLoading = $state(false);
	let privilegesError = $state<string | null>(null);
	let objectGrants = $state<Grants>({});
	let originalObjectGrants = $state.raw<Grants>({});
	let filter = $state('');
	let onlyWithAccess = $state(false);

	let reviewing = $state(false);
	let busy = $state(false);
	let applyError = $state<string | null>(null);
	let sql = $state<string[]>([]);

	let readOnly = $derived(connections.get(tab.connectionId)?.readOnly !== false);
	let locked = $derived(readOnly || busy);
	let planned = $derived(
		details && draft
			? planChanges(
					details,
					draft,
					{ ...originalDatabaseGrants, ...originalObjectGrants },
					{ ...databaseGrants, ...objectGrants }
				)
			: null
	);
	let dirty = $derived(planned?.count ?? 0);
	let objectChanges = $derived(
		planned?.changes.privileges?.filter((p) => p.target.kind !== 'database').length ?? 0
	);
	let tables = $derived(
		privileges ? filterObjects(privileges.relations, filter, onlyWithAccess, objectGrants) : []
	);
	let membershipCandidates = $derived(
		details && draft
			? details.otherRoles
					.filter((r) => !draft!.memberOf.includes(r))
					// Predefined pg_* roles are useful but numerous, so they go last.
					.sort((a, b) => Number(a.startsWith('pg_')) - Number(b.startsWith('pg_')))
			: []
	);
	let summary = $derived.by(() => {
		if (!details) return '';
		const parts = [details.canLogin ? 'Can log in' : 'Cannot log in'];
		if (details.memberOf.length)
			parts.push(`member of ${details.memberOf.map((m) => m.role).join(', ')}`);
		if (details.members.length)
			parts.push(`${details.members.length} member${details.members.length === 1 ? '' : 's'}`);
		if (details.validUntil) parts.push(`expires ${details.validUntil}`);
		return parts.join(' · ');
	});

	$effect(() => {
		tab.dirty = dirty;
	});
	$effect(() => {
		if (section === 'privileges' && !privileges && !privilegesLoading && !privilegesError)
			void loadPrivileges();
	});

	onMount(() => void load());

	async function load() {
		loading = true;
		error = null;
		try {
			const role = await api.roleDetails(tab.connectionId, tab.role);
			details = role;
			draft = draftOf(role);
			originalDatabaseGrants = grantsOf(role.databases);
			databaseGrants = grantsOf(role.databases);
			if (!role.databases.some((d) => d.name === tab.database) && role.databases.length)
				tab.database = role.databases[0].name;
			if (privileges || section === 'privileges') await loadPrivileges();
		} catch (e) {
			error = errorMessage(e);
		} finally {
			loading = false;
		}
	}

	async function loadPrivileges() {
		privilegesLoading = true;
		privilegesError = null;
		try {
			const loaded = await api.rolePrivileges(tab.connectionId, tab.database, tab.role);
			privileges = loaded;
			const grants = grantsOf([...loaded.schemas, ...loaded.relations]);
			originalObjectGrants = grants;
			objectGrants = { ...grants };
		} catch (e) {
			privileges = null;
			privilegesError = errorMessage(e);
		} finally {
			privilegesLoading = false;
		}
	}

	async function refresh() {
		if (!(await confirmDiscard(dirty))) return;
		privileges = null;
		await load();
	}

	async function switchDatabase(database: string) {
		if (database === tab.database) return;
		if (!(await confirmDiscard(objectChanges))) return;
		tab.database = database;
		privileges = null;
		await loadPrivileges();
	}

	function discard() {
		if (!details) return;
		draft = draftOf(details);
		databaseGrants = { ...originalDatabaseGrants };
		objectGrants = { ...originalObjectGrants };
	}

	async function review() {
		if (!planned || planned.problem || !dirty || busy) return;
		busy = true;
		applyError = null;
		try {
			sql = await api.previewRoleChanges(planned.changes);
			reviewing = true;
		} catch (e) {
			applyError = errorMessage(e);
		} finally {
			busy = false;
		}
	}

	async function apply() {
		if (!planned || busy) return;
		busy = true;
		applyError = null;
		try {
			await api.applyRoleChanges(tab.connectionId, tab.database, planned.changes);
			if (planned.changes.rename) {
				tab.role = planned.changes.rename;
				tab.title = planned.changes.rename;
			}
			reviewing = false;
			roleEvents.changed();
			await load();
		} catch (e) {
			applyError = errorMessage(e);
		} finally {
			busy = false;
		}
	}

	async function drop() {
		if (!details || readOnly) return;
		const ok = await confirmation.ask({
			title: `Drop role ${details.name}?`,
			description:
				'This runs DROP ROLE. It fails while the role still owns objects or holds privileges in any database; reassign or revoke those first.',
			confirmLabel: 'Drop role',
			destructive: true
		});
		if (!ok) return;
		busy = true;
		error = null;
		try {
			await api.dropRole(tab.connectionId, details.name);
			roleEvents.changed();
			tabs.closeRole(tab.connectionId, details.name);
		} catch (e) {
			error = errorMessage(e);
		} finally {
			busy = false;
		}
	}

	function toggle(object: ObjectPrivileges, privilege: string) {
		const key = targetKey(targetOf(object));
		if (object.kind === 'database') databaseGrants = toggleGrant(databaseGrants, key, privilege);
		else objectGrants = toggleGrant(objectGrants, key, privilege);
	}

	function toggleAll(objects: ObjectPrivileges[], privilege: string) {
		const keys = objects.filter((o) => !o.owner).map((o) => targetKey(targetOf(o)));
		if (!keys.length) return;
		objectGrants = toggleColumn(objectGrants, keys, privilege);
	}

	function openRole(name: string) {
		tabs.openRole(tab.connectionId, tab.database, name);
	}
</script>

{#snippet grantCell(object: ObjectPrivileges, privilege: string)}
	{@const key = targetKey(targetOf(object))}
	{@const grants = object.kind === 'database' ? databaseGrants : objectGrants}
	{@const original = object.kind === 'database' ? originalDatabaseGrants : originalObjectGrants}
	{@const granted = grants[key]?.includes(privilege) ?? false}
	{@const changed = granted !== (original[key]?.includes(privilege) ?? false)}
	{@const inherited =
		!object.owner &&
		!granted &&
		!original[key]?.includes(privilege) &&
		object.effective.includes(privilege)}
	<td
		class="px-2 py-1 text-center"
		class:inherited
		class:bg-signal-soft={changed}
		title={object.owner
			? 'Owners have every privilege'
			: inherited
				? 'Allowed through PUBLIC or a member role'
				: undefined}
	>
		<input
			type="checkbox"
			class="grant"
			checked={granted || object.owner}
			disabled={locked || object.owner}
			aria-label="{privilege} on {object.schema ? `${object.schema}.` : ''}{object.name}"
			onchange={() => toggle(object, privilege)}
		/>
	</td>
{/snippet}

{#snippet objectName(object: ObjectPrivileges)}
	<td class="max-w-0 py-1 pr-3 pl-4">
		<div class="flex min-w-0 items-center gap-2">
			<span class="measured truncate"
				>{#if object.schema}<span class="text-muted-foreground">{object.schema}.</span
					>{/if}{object.name}</span
			>
			{#if object.kind !== 'table' && object.kind !== 'database' && object.kind !== 'schema'}
				<span class="text-[11px] text-muted-foreground">{object.kind}</span>
			{/if}
			{#if object.owner}<span class="chip">owner</span>{/if}
		</div>
	</td>
{/snippet}

{#snippet sectionTitle(title: string, hint?: string)}
	<div class="flex flex-col gap-0.5">
		<h3 class="text-[13px] font-medium">{title}</h3>
		{#if hint}<p class="text-xs text-muted-foreground">{hint}</p>{/if}
	</div>
{/snippet}

<div class="flex h-full min-h-0 flex-col bg-background text-sm">
	{#if loading && !details}
		<div class="flex flex-1 items-center justify-center gap-2 text-muted-foreground">
			<LoaderIcon class="size-4 animate-spin" /> Loading role…
		</div>
	{:else if !details || !draft}
		<div class="flex flex-1 flex-col items-center justify-center gap-3 px-10">
			<p class="max-w-md text-center text-bad" role="alert">
				{error ?? 'The role could not be loaded.'}
			</p>
			<Button size="sm" variant="outline" onclick={load}><RefreshIcon /> Try again</Button>
		</div>
	{:else}
		<header class="flex shrink-0 items-center gap-3 border-b bg-chrome px-4 py-3">
			<div
				class="flex size-9 shrink-0 items-center justify-center rounded-lg border bg-background text-muted-foreground"
			>
				{#if details.superuser}
					<ShieldUserIcon class="text-accent-text size-4.5" />
				{:else if details.canLogin}
					<UserIcon class="size-4.5" />
				{:else}
					<UsersIcon class="size-4.5" />
				{/if}
			</div>
			<div class="flex min-w-0 flex-1 flex-col gap-0.5">
				<div class="flex min-w-0 items-center gap-2">
					<h2 class="measured truncate text-[15px] font-medium">{details.name}</h2>
					{#if details.superuser}<span class="chip chip-signal">superuser</span>{/if}
					{#if !details.canLogin}<span class="chip">group role</span>{/if}
				</div>
				<p class="truncate text-xs text-muted-foreground" title={summary}>{summary}</p>
			</div>
			<Button
				size="icon-sm"
				variant="ghost"
				title="Reload"
				aria-label="Reload"
				disabled={busy || loading}
				onclick={refresh}
			>
				<RefreshIcon class={loading ? 'animate-spin' : ''} />
			</Button>
			{#if !readOnly}
				<Button size="sm" variant="ghost" class="text-bad" disabled={busy} onclick={drop}>
					<TrashIcon /> Drop
				</Button>
			{/if}
		</header>

		<div
			class="flex h-10 shrink-0 items-center gap-1 border-b px-3"
			role="tablist"
			aria-label="Role"
		>
			{#each SECTIONS as s (s.id)}
				<Button
					role="tab"
					aria-selected={section === s.id}
					variant={section === s.id ? 'secondary' : 'ghost'}
					size="sm"
					onclick={() => (section = s.id)}
				>
					{s.label}
					{#if s.id === 'membership' && draft.memberOf.length + details.members.length}
						<span class="measured text-muted-foreground"
							>{draft.memberOf.length + details.members.length}</span
						>
					{/if}
				</Button>
			{/each}
		</div>

		{#if error}
			<p
				class="flex items-start gap-2 border-b bg-bad-soft px-4 py-2 text-xs text-bad"
				role="alert"
			>
				<TriangleAlertIcon class="mt-px size-3.5 shrink-0" />{error}
			</p>
		{/if}

		<div class="min-h-0 flex-1 overflow-auto">
			{#if section === 'general'}
				<div class="grid max-w-3xl gap-8 p-5">
					<section class="grid gap-3">
						{@render sectionTitle('Identity')}
						<div class="grid gap-3 sm:grid-cols-2">
							<label class="grid gap-1.5 text-xs text-muted-foreground"
								>Name
								<Input
									class="measured"
									bind:value={draft.name}
									disabled={locked}
									autocomplete="off"
									aria-invalid={!draft.name.trim()}
								/>
							</label>
							<label class="grid gap-1.5 text-xs text-muted-foreground"
								>Comment
								<Input
									bind:value={draft.comment}
									disabled={locked}
									placeholder="What this role is for"
								/>
							</label>
						</div>
					</section>

					<section class="grid gap-3">
						{@render sectionTitle('Sign-in')}
						<label class="flex items-center gap-2.5">
							<Checkbox bind:checked={draft.canLogin} disabled={locked} />
							<span>Can log in</span>
							<span class="text-xs text-muted-foreground"
								>Without login this is a group role that only bundles privileges.</span
							>
						</label>
						<div class="grid gap-3 sm:grid-cols-3">
							<label class="grid gap-1.5 text-xs text-muted-foreground"
								>New password
								<Input
									type="password"
									bind:value={draft.password}
									disabled={locked || draft.removePassword}
									placeholder="Unchanged"
									autocomplete="new-password"
								/>
							</label>
							<label class="grid gap-1.5 text-xs text-muted-foreground"
								>Password expires
								<Input
									class="measured"
									bind:value={draft.validUntil}
									disabled={locked}
									placeholder="Never, or 2027-12-31"
								/>
							</label>
							<label class="grid gap-1.5 text-xs text-muted-foreground"
								>Connection limit
								<Input
									class="measured"
									inputmode="numeric"
									bind:value={draft.connectionLimit}
									disabled={locked}
									placeholder="Unlimited"
									aria-invalid={!!draft.connectionLimit.trim() &&
										!/^\d+$/.test(draft.connectionLimit.trim())}
								/>
							</label>
						</div>
						<label class="flex items-center gap-2.5 text-xs text-muted-foreground">
							<Checkbox bind:checked={draft.removePassword} disabled={locked || !!draft.password} />
							Remove the password (password sign-in stops working)
						</label>
					</section>

					<section class="grid gap-3">
						{@render sectionTitle('Capabilities')}
						<div class="grid gap-px overflow-hidden rounded-lg border bg-border">
							{#each CAPABILITIES as capability (capability.key)}
								<label
									class="flex items-center gap-3 bg-background px-3 py-2.5 hover:bg-hover"
									class:opacity-60={locked}
								>
									<Checkbox
										checked={draft[capability.key] as boolean}
										onCheckedChange={(v) => ((draft![capability.key] as boolean) = v)}
										disabled={locked}
									/>
									<span class="flex min-w-0 flex-col">
										<span>{capability.label}</span>
										<span class="text-xs text-muted-foreground">{capability.hint}</span>
									</span>
								</label>
							{/each}
						</div>
					</section>
				</div>
			{:else if section === 'membership'}
				<div class="grid max-w-3xl gap-8 p-5">
					<section class="grid gap-3">
						{@render sectionTitle(
							'Member of',
							draft.inherit
								? `${details.name} gets the privileges of these roles.`
								: `${details.name} can SET ROLE to these roles, but does not inherit their privileges.`
						)}
						<div class="overflow-hidden rounded-lg border">
							{#each draft.memberOf as group (group)}
								{@const original = details.memberOf.find((m) => m.role === group)}
								<div class="flex h-9 items-center gap-2 border-b px-3 last:border-b-0">
									<UsersIcon class="size-3.5 shrink-0 text-muted-foreground" />
									<button
										type="button"
										class="measured truncate hover:underline"
										onclick={() => openRole(group)}>{group}</button
									>
									{#if original?.admin}<span class="chip" title="Can grant this role to others"
											>admin</span
										>{/if}
									{#if !original}<span class="chip chip-signal">new</span>{/if}
									<Button
										class="ml-auto"
										size="icon-xs"
										variant="ghost"
										title="Remove membership"
										aria-label="Remove membership in {group}"
										disabled={locked}
										onclick={() => (draft!.memberOf = draft!.memberOf.filter((r) => r !== group))}
									>
										<XIcon />
									</Button>
								</div>
							{:else}
								<p class="px-3 py-2.5 text-muted-foreground">Not a member of any role.</p>
							{/each}
						</div>
						{#if !readOnly}
							<Select.Root
								type="single"
								value=""
								onValueChange={(v) => {
									if (v) draft!.memberOf = [...draft!.memberOf, v];
								}}
								disabled={locked || !membershipCandidates.length}
							>
								<Select.Trigger size="sm" class="w-64">Add membership…</Select.Trigger>
								<Select.Content class="max-h-80">
									{#each membershipCandidates as candidate (candidate)}
										<Select.Item value={candidate} label={candidate} />
									{/each}
								</Select.Content>
							</Select.Root>
						{/if}
					</section>

					<section class="grid gap-3">
						{@render sectionTitle('Members', `Roles that are members of ${details.name}.`)}
						<div class="overflow-hidden rounded-lg border">
							{#each details.members as member (member.role)}
								<div class="flex h-9 items-center gap-2 border-b px-3 last:border-b-0">
									<UserIcon class="size-3.5 shrink-0 text-muted-foreground" />
									<button
										type="button"
										class="measured truncate hover:underline"
										onclick={() => openRole(member.role)}>{member.role}</button
									>
									{#if member.admin}<span class="chip">admin</span>{/if}
								</div>
							{:else}
								<p class="px-3 py-2.5 text-muted-foreground">No members.</p>
							{/each}
						</div>
					</section>
				</div>
			{:else}
				<div class="grid gap-8 p-5">
					{#if details.superuser}
						<p
							class="flex max-w-3xl items-start gap-2 rounded-lg border bg-signal-soft px-3 py-2 text-xs"
						>
							<ShieldUserIcon class="mt-px size-3.5 shrink-0" />
							Superusers skip every privilege check, so the grants below have no effect for {details.name}.
						</p>
					{/if}
					<p class="flex items-center gap-2 text-xs text-muted-foreground">
						<span class="inherited inline-block size-3 rounded-sm border"></span>
						Allowed without a direct grant, through PUBLIC or a member role.
					</p>

					<section class="grid max-w-3xl gap-3">
						{@render sectionTitle('Databases')}
						<div class="overflow-hidden rounded-lg border">
							<table class="w-full table-fixed text-[13px]">
								<thead class="bg-chrome text-xs text-muted-foreground">
									<tr class="border-b">
										<th class="py-1.5 pl-4 text-left font-normal">Database</th>
										{#each DATABASE_PRIVILEGES as privilege (privilege)}
											<th class="w-24 py-1.5 font-normal">{label(privilege)}</th>
										{/each}
									</tr>
								</thead>
								<tbody>
									{#each details.databases as database (database.name)}
										<tr class="border-b last:border-b-0 hover:bg-hover">
											{@render objectName(database)}
											{#each DATABASE_PRIVILEGES as privilege (privilege)}
												{@render grantCell(database, privilege)}
											{/each}
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					</section>

					<section class="grid gap-3">
						<div class="flex flex-wrap items-center gap-2">
							<h3 class="text-[13px] font-medium">Schemas and tables in</h3>
							<Select.Root
								type="single"
								value={tab.database}
								onValueChange={(v) => v && switchDatabase(v)}
								disabled={busy || privilegesLoading}
							>
								<Select.Trigger size="sm" class="measured min-w-40">{tab.database}</Select.Trigger>
								<Select.Content>
									{#each details.databases as database (database.name)}
										<Select.Item value={database.name} label={database.name} />
									{/each}
								</Select.Content>
							</Select.Root>
							{#if privilegesLoading}<LoaderIcon
									class="size-3.5 animate-spin text-muted-foreground"
								/>{/if}
						</div>

						{#if privilegesError}
							<p class="text-xs text-bad" role="alert">{privilegesError}</p>
						{:else if privileges}
							<div class="max-w-3xl overflow-hidden rounded-lg border">
								<table class="w-full table-fixed text-[13px]">
									<thead class="bg-chrome text-xs text-muted-foreground">
										<tr class="border-b">
											<th class="py-1.5 pl-4 text-left font-normal">Schema</th>
											{#each SCHEMA_PRIVILEGES as privilege (privilege)}
												<th class="w-24 py-1.5 font-normal">{label(privilege)}</th>
											{/each}
										</tr>
									</thead>
									<tbody>
										{#each privileges.schemas as schema (schema.name)}
											<tr class="border-b last:border-b-0 hover:bg-hover">
												{@render objectName(schema)}
												{#each SCHEMA_PRIVILEGES as privilege (privilege)}
													{@render grantCell(schema, privilege)}
												{/each}
											</tr>
										{/each}
									</tbody>
								</table>
							</div>

							<div class="flex flex-wrap items-center gap-3">
								<Input class="h-8 w-64" bind:value={filter} placeholder="Filter tables" />
								<label class="flex items-center gap-2 text-xs text-muted-foreground">
									<Checkbox bind:checked={onlyWithAccess} /> Only tables with access
								</label>
								<span class="measured ml-auto text-xs text-muted-foreground"
									>{tables.length} of {privileges.relations.length}</span
								>
							</div>
							<div class="overflow-hidden rounded-lg border">
								<table class="w-full min-w-[44rem] table-fixed text-[13px]">
									<thead class="bg-chrome text-xs text-muted-foreground">
										<tr class="border-b">
											<th class="py-1.5 pl-4 text-left font-normal">Table</th>
											{#each TABLE_PRIVILEGES as privilege (privilege)}
												<th class="w-24 py-1 font-normal">
													<button
														type="button"
														class="rounded px-1.5 py-0.5 hover:bg-hover hover:text-foreground disabled:pointer-events-none"
														title="Toggle {privilege} on all {tables.length} shown tables"
														disabled={locked || !tables.length}
														onclick={() => toggleAll(tables, privilege)}>{label(privilege)}</button
													>
												</th>
											{/each}
										</tr>
									</thead>
									<tbody>
										{#each tables as table (`${table.schema}.${table.name}`)}
											<tr class="border-b last:border-b-0 hover:bg-hover">
												{@render objectName(table)}
												{#each TABLE_PRIVILEGES as privilege (privilege)}
													{@render grantCell(table, privilege)}
												{/each}
											</tr>
										{:else}
											<tr>
												<td
													colspan={TABLE_PRIVILEGES.length + 1}
													class="px-4 py-2.5 text-muted-foreground"
												>
													{privileges.relations.length ? 'No matching tables.' : 'No tables.'}
												</td>
											</tr>
										{/each}
									</tbody>
								</table>
							</div>
						{/if}
					</section>
				</div>
			{/if}
		</div>

		<footer class="flex h-9 shrink-0 items-center gap-1 border-t bg-chrome px-2 text-xs">
			{#if readOnly}
				<span class="flex items-center gap-1.5 px-1 text-muted-foreground"
					><LockIcon class="size-3.5" />Read-only connection.</span
				>
			{:else if dirty || planned?.problem}
				{#if planned?.problem}
					<span class="px-1 text-bad">{planned.problem}</span>
				{:else if applyError && !reviewing}
					<span class="truncate px-1 text-bad" title={applyError}>{applyError}</span>
				{/if}
				<span class="ml-auto px-1 font-medium text-signal-text">
					<span class="measured">{dirty}</span> pending change{dirty === 1 ? '' : 's'}
				</span>
				<Button size="xs" variant="ghost" disabled={busy} onclick={discard}>Discard</Button>
				<Button size="xs" disabled={busy || !!planned?.problem || !dirty} onclick={review}>
					{#if busy}<LoaderIcon class="animate-spin" />{/if}
					Review & apply
				</Button>
			{:else}
				<span class="px-1 text-muted-foreground">Nothing runs until you review and apply.</span>
			{/if}
		</footer>
	{/if}
</div>

<Dialog.Root
	open={reviewing}
	onOpenChange={(open) => {
		if (!open && !busy) reviewing = false;
	}}
>
	<Dialog.Content class="sm:max-w-3xl" showCloseButton={!busy}>
		<Dialog.Header>
			<Dialog.Title>Review changes</Dialog.Title>
			<Dialog.Description>
				{dirty} change{dirty === 1 ? '' : 's'} to role
				<span class="measured">{details?.name}</span>, run in one transaction. Passwords are sent as
				a SCRAM hash, never in plain text.
			</Dialog.Description>
		</Dialog.Header>
		{#if applyError}<p role="alert" class="text-xs text-bad">{applyError}</p>{/if}
		<pre
			class="measured max-h-[55vh] overflow-auto rounded-lg border bg-background p-3 text-xs whitespace-pre-wrap select-text">{#each sql as statement, n (n)}{#each highlightSql(statement + ';') as token, t (t)}{#if token.classes}<span
							class={token.classes}>{token.text}</span
						>{:else}{token.text}{/if}{/each}{n < sql.length - 1 ? '\n' : ''}{/each}</pre>
		<Dialog.Footer>
			<Button variant="outline" disabled={busy} onclick={() => (reviewing = false)}>Cancel</Button>
			<Button disabled={busy} onclick={apply}>
				{#if busy}<LoaderIcon class="animate-spin" />{/if}
				Apply {dirty} change{dirty === 1 ? '' : 's'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<style>
	.grant {
		width: 0.875rem;
		height: 0.875rem;
		accent-color: var(--color-action);
		cursor: pointer;
		vertical-align: middle;
	}
	.inherited {
		background: color-mix(in oklab, var(--color-ok) 18%, transparent);
	}
	.grant:disabled {
		cursor: default;
		opacity: 0.55;
	}
</style>
