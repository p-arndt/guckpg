<script lang="ts">
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import FingerprintIcon from '@lucide/svelte/icons/fingerprint';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import LinkIcon from '@lucide/svelte/icons/link';
	import ListChecksIcon from '@lucide/svelte/icons/list-checks';
	import LockIcon from '@lucide/svelte/icons/lock';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import TrashIcon from '@lucide/svelte/icons/trash';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import * as Select from '#lib/components/ui/select/index.js';
	import {
		FK_ACTION_KEYS,
		NEW_CONSTRAINT_KINDS,
		REFERENTIAL_ACTIONS,
		columnRenames,
		namedStatus,
		referenceTables,
		restored,
		toggleConstraintColumn,
		type AutoNamed,
		type ChangeStatus,
		type NewConstraintKind,
		type ReferenceOption,
		type StructureConstraint,
		type TableStructure
	} from '#lib/structure.js';
	import CheckBuilder from '../CheckBuilder.svelte';
	import ColumnChips from './ColumnChips.svelte';
	import RemovedRow from './RemovedRow.svelte';
	import StatusChip from './StatusChip.svelte';

	interface Props {
		draft: TableStructure;
		columnNames: (string | null)[];
		original: TableStructure | null;
		status: ChangeStatus;
		removed: StructureConstraint[];
		schema: string;
		schemas: string[];
		relations: Record<string, { name: string }[]>;
		columnList: string[];
		autoNamed: AutoNamed & Pick<WeakSet<object>, 'delete'>;
		locked: boolean;
		changed: () => void;
		attempt: (change: () => void) => void;
		referenceOptions: (constraint: StructureConstraint) => ReferenceOption[];
		onAdd: (kind: NewConstraintKind) => void;
		onRefSchema: (constraint: StructureConstraint, schema: string) => void;
		onRefTable: (constraint: StructureConstraint, table: string) => void;
	}
	let {
		draft = $bindable(),
		columnNames,
		original,
		status,
		removed,
		schema,
		schemas,
		relations,
		columnList,
		autoNamed,
		locked,
		changed,
		attempt,
		referenceOptions,
		onAdd,
		onRefSchema,
		onRefTable
	}: Props = $props();

	const KINDS = {
		primaryKey: { label: 'Primary key', short: 'PK', icon: KeyRoundIcon },
		unique: { label: 'Unique', short: 'Unique', icon: FingerprintIcon },
		foreignKey: { label: 'Foreign key', short: 'FK', icon: LinkIcon },
		check: { label: 'Check', short: 'Check', icon: ListChecksIcon },
		other: { label: 'Other', short: 'Other', icon: LockIcon }
	} as const;
</script>

<div class="space-y-2 p-3">
	{#each draft.constraints as c, i (c)}
		{@const s = original
			? namedStatus(c.name, original.constraints, status.addedConstraints)
			: 'same'}
		{@const Icon = KINDS[c.kind].icon}
		<section class="item {s !== 'same' ? `item-${s}` : ''}">
			<div class="flex items-center gap-2">
				<span class="chip shrink-0" title={KINDS[c.kind].label}
					><Icon class="size-3" />{KINDS[c.kind].short}</span
				>
				{#if c.kind === 'other'}
					<span class="measured truncate">{c.name}</span>
					<span class="flex-1"></span>
					<span class="chip"><LockIcon class="size-3" /> Read-only</span>
				{:else}
					<input
						class="cell measured max-w-80"
						value={c.name}
						aria-label="Constraint name"
						aria-invalid={!c.name}
						oninput={(e) => {
							c.name = e.currentTarget.value;
							autoNamed.delete(c);
						}}
					/>
					<span class="flex-1"></span>
					<StatusChip status={s} />
					<Button
						size="icon-xs"
						variant="ghost"
						disabled={locked}
						title="Remove constraint"
						aria-label="Remove constraint {c.name}"
						onclick={() => {
							draft.constraints.splice(i, 1);
							changed();
						}}><TrashIcon /></Button
					>
				{/if}
			</div>
			{#if c.kind === 'other'}
				<p class="measured mt-2 text-xs break-all text-muted-foreground">
					{c.definition}
				</p>
			{:else if c.kind === 'check'}
				<div class="mt-2 space-y-2">
					<input
						class="field measured"
						value={c.expression ?? ''}
						placeholder="price > 0"
						aria-label="Check expression"
						aria-invalid={!c.expression}
						oninput={(e) => (c.expression = e.currentTarget.value || null)}
					/>
					<CheckBuilder
						columns={columnList}
						disabled={locked}
						onapply={(expression) => {
							c.expression = expression;
							changed();
						}}
					/>
				</div>
			{:else}
				<div class="mt-2 space-y-2">
					<div class="flex items-baseline gap-3">
						<span class="label">Columns</span>
						<ColumnChips
							columns={columnList}
							selected={c.columns}
							disabled={locked}
							ordered
							toggle={(name) => {
								toggleConstraintColumn(draft, c, name, referenceOptions(c), autoNamed);
								changed();
							}}
						/>
					</div>
					{#if c.kind === 'foreignKey'}
						<div class="flex flex-wrap items-center gap-2">
							<span class="label">References</span>
							<Select.Root
								type="single"
								value={c.refSchema ?? schema}
								onValueChange={(v) => onRefSchema(c, v)}
								disabled={locked}
							>
								<Select.Trigger size="sm" class="measured min-w-28" aria-label="Schema"
									>{c.refSchema ?? schema}</Select.Trigger
								>
								<Select.Content>
									{#each schemas as name (name)}<Select.Item value={name} label={name} />{/each}
								</Select.Content>
							</Select.Root>
							<span class="text-muted-foreground">.</span>
							<Select.Root
								type="single"
								value={c.refTable ?? ''}
								onValueChange={(v) => onRefTable(c, v)}
								disabled={locked}
							>
								<Select.Trigger
									size="sm"
									class="measured min-w-40 {c.refTable ? '' : 'text-muted-foreground'}"
									aria-label="Referenced table"
									aria-invalid={!c.refTable}>{c.refTable ?? 'Choose a table'}</Select.Trigger
								>
								<Select.Content class="max-h-72">
									{#each referenceTables(draft, schema, relations, c) as name (name)}<Select.Item
											value={name}
											label={name}
											>{name}{#if name === draft.name && (c.refSchema || schema) === schema}<span
													class="text-muted-foreground">(this table)</span
												>{/if}</Select.Item
										>{/each}
								</Select.Content>
							</Select.Root>
						</div>
						{#if c.columns.length && c.refTable}
							<div class="ml-[5.5rem] space-y-1">
								{#each c.columns as name, n (name)}
									<div class="flex items-center gap-2 text-xs">
										<span class="measured w-36 truncate">{name}</span>
										<span class="text-muted-foreground">→</span>
										<Select.Root
											type="single"
											value={c.refColumns[n] ?? ''}
											onValueChange={(v) => {
												const next = [...c.refColumns];
												next[n] = v;
												c.refColumns = next;
												changed();
											}}
											disabled={locked}
										>
											<Select.Trigger
												size="sm"
												class="measured min-w-40 {c.refColumns[n] ? '' : 'text-muted-foreground'}"
												aria-label="Referenced column for {name}"
												aria-invalid={!c.refColumns[n]}
												>{c.refColumns[n] || 'Choose a column'}</Select.Trigger
											>
											<Select.Content class="max-h-72">
												{#each referenceOptions(c) as option (option.name)}<Select.Item
														value={option.name}
														label={option.name}
														>{option.name}{#if option.primaryKey}<KeyRoundIcon
																class="size-3 text-muted-foreground"
															/>{/if}</Select.Item
													>{/each}
											</Select.Content>
										</Select.Root>
									</div>
								{/each}
							</div>
						{/if}
						<div class="flex flex-wrap items-center gap-4">
							{#each FK_ACTION_KEYS as action (action)}
								<div class="flex items-center gap-2">
									<span class="label">{action === 'onUpdate' ? 'On update' : 'On delete'}</span>
									<Select.Root
										type="single"
										value={c[action] ?? 'NO ACTION'}
										onValueChange={(v) => {
											if (v === (c[action] ?? 'NO ACTION')) return;
											c[action] = v;
											changed();
										}}
										disabled={locked}
									>
										<Select.Trigger size="sm" class="measured min-w-32 text-xs"
											>{c[action] ?? 'NO ACTION'}</Select.Trigger
										>
										<Select.Content>
											{#each REFERENTIAL_ACTIONS as a (a)}<Select.Item
													value={a}
													label={a}
													class="measured text-xs"
												/>{/each}
										</Select.Content>
									</Select.Root>
								</div>
							{/each}
						</div>
					{/if}
				</div>
			{/if}
		</section>
	{:else}
		{#if !removed.length}
			<p class="py-6 text-center text-muted-foreground">
				No constraints. Add a primary key, unique, foreign key or check.
			</p>
		{/if}
	{/each}
	{#each removed as c (c.name)}
		<RemovedRow
			label={c.name}
			detail={c.definition}
			disabled={locked}
			onrestore={() =>
				attempt(() => draft.constraints.push(restored(c, columnRenames(draft, columnNames))))}
		/>
	{/each}
	<DropdownMenu.Root>
		<DropdownMenu.Trigger disabled={locked}>
			{#snippet child({ props })}
				<Button {...props} size="sm" variant="outline"
					><PlusIcon /> Add constraint <ChevronDownIcon /></Button
				>
			{/snippet}
		</DropdownMenu.Trigger>
		<DropdownMenu.Content align="start" class="w-48">
			{#each NEW_CONSTRAINT_KINDS as kind (kind)}
				{@const Icon = KINDS[kind].icon}
				<DropdownMenu.Item
					disabled={kind === 'primaryKey' && draft.constraints.some((c) => c.kind === 'primaryKey')}
					onSelect={() => onAdd(kind)}
				>
					<Icon />{KINDS[kind].label}
				</DropdownMenu.Item>
			{/each}
		</DropdownMenu.Content>
	</DropdownMenu.Root>
</div>
