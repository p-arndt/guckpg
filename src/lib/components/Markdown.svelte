<script lang="ts">
	import CopyIcon from '@lucide/svelte/icons/copy';
	import CheckIcon from '@lucide/svelte/icons/check';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import { Button } from '#lib/components/ui/button/index.js';
	import { highlightSql } from '#lib/highlight.js';
	import { isSqlBlock, parseMarkdown, type Block, type Inline } from '#lib/markdown.js';

	interface Props {
		text: string;
		/** Opens a SQL block in a new editor tab; without it, SQL blocks only offer Copy. */
		onOpenSql?: (sql: string) => void;
		canOpenSql?: boolean;
	}

	let { text, onOpenSql, canOpenSql = true }: Props = $props();

	const blocks = $derived(parseMarkdown(text));
	let copied = $state<string | null>(null);

	async function copy(code: string, key: string) {
		await navigator.clipboard.writeText(code);
		copied = key;
		setTimeout(() => {
			if (copied === key) copied = null;
		}, 1500);
	}

	const SQL_LANG = /^(sql|postgresql|postgres|pgsql|psql)$/i;
</script>

{#snippet inline(nodes: Inline[])}
	{#each nodes as node, i (i)}
		{#if node.type === 'text'}{node.text}{:else if node.type === 'break'}<br
			/>{:else if node.type === 'code'}<code
				class="rounded-sm bg-hover px-1 py-px text-[0.92em] text-foreground">{node.text}</code
			>{:else if node.type === 'strong'}<strong class="font-semibold text-foreground"
				>{@render inline(node.children)}</strong
			>{:else if node.type === 'em'}<em>{@render inline(node.children)}</em
			>{:else if node.type === 'del'}<del class="text-muted-foreground"
				>{@render inline(node.children)}</del
			>{:else if node.type === 'link'}<span
				class="text-foreground underline decoration-rule-2 underline-offset-2"
				title={node.href}>{@render inline(node.children)}</span
			>{/if}
	{/each}
{/snippet}

{#snippet code(block: Extract<Block, { type: 'code' }>, key: string)}
	{@const sql = isSqlBlock(block)}
	<figure class="border-rule overflow-hidden rounded-lg border">
		<figcaption class="border-rule flex h-8 items-center gap-1 border-b bg-chrome pr-1 pl-3">
			<span class="text-xs text-muted-foreground">{block.lang.toLowerCase() || 'code'}</span>
			<Button
				size="xs"
				variant="ghost"
				class="ml-auto"
				aria-label="Copy code"
				onclick={() => copy(block.text, key)}
			>
				{#if copied === key}<CheckIcon /> Copied{:else}<CopyIcon /> Copy{/if}
			</Button>
			{#if sql && onOpenSql}
				<Button
					size="xs"
					variant="outline"
					disabled={!canOpenSql}
					onclick={() => onOpenSql(block.text.trim())}
				>
					<SquareTerminalIcon /> Open in editor
				</Button>
			{/if}
		</figcaption>
		<pre class="overflow-x-auto bg-background px-3 py-2.5 text-[12.5px] leading-[1.6]"><code
				>{#if SQL_LANG.test(block.lang)}{#each highlightSql(block.text) as token, t (t)}{#if token.classes}<span
								class={token.classes}>{token.text}</span
							>{:else}{token.text}{/if}{/each}{:else}{block.text}{/if}</code
			></pre>
	</figure>
{/snippet}

{#snippet render(nodes: Block[], path: string)}
	{#each nodes as block, i (i)}
		{@const key = `${path}.${i}`}
		{#if block.type === 'paragraph'}
			<p>{@render inline(block.children)}</p>
		{:else if block.type === 'heading'}
			{#if block.level <= 2}
				<h3 class="mt-1 text-[15px] text-foreground">{@render inline(block.children)}</h3>
			{:else}
				<h4 class="mt-1 font-medium text-foreground">{@render inline(block.children)}</h4>
			{/if}
		{:else if block.type === 'code'}
			{@render code(block, key)}
		{:else if block.type === 'list'}
			{#if block.ordered}
				<ol
					class="flex list-decimal flex-col gap-1 pl-5 marker:text-muted-foreground"
					start={block.start}
				>
					{#each block.items as item, j (j)}
						<li class="pl-0.5 [&>*+*]:mt-1.5">{@render render(item, `${key}.${j}`)}</li>
					{/each}
				</ol>
			{:else}
				<ul class="flex list-disc flex-col gap-1 pl-5 marker:text-muted-foreground">
					{#each block.items as item, j (j)}
						<li class="pl-0.5 [&>*+*]:mt-1.5">{@render render(item, `${key}.${j}`)}</li>
					{/each}
				</ul>
			{/if}
		{:else if block.type === 'quote'}
			<blockquote class="flex flex-col gap-2 border-l-2 border-rule-2 pl-3 text-muted-foreground">
				{@render render(block.children, key)}
			</blockquote>
		{:else if block.type === 'table'}
			<div class="border-rule overflow-x-auto rounded-lg border">
				<table class="w-full border-collapse text-[13px]">
					<thead class="bg-chrome">
						<tr>
							{#each block.head as cell, c (c)}
								<th
									class="border-rule border-b px-2.5 py-1.5 font-medium whitespace-nowrap text-muted-foreground"
									style:text-align={block.align[c] ?? 'left'}>{@render inline(cell)}</th
								>
							{/each}
						</tr>
					</thead>
					<tbody>
						{#each block.rows as row, r (r)}
							<tr class="border-rule/70 border-b last:border-b-0">
								{#each row as cell, c (c)}
									<td
										class="px-2.5 py-1.5 align-top tabular-nums"
										style:text-align={block.align[c] ?? 'left'}>{@render inline(cell)}</td
									>
								{/each}
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{:else if block.type === 'rule'}
			<hr class="border-rule" />
		{/if}
	{/each}
{/snippet}

<div class="flex min-w-0 flex-col gap-2.5 leading-[1.6] break-words text-ink-2">
	{@render render(blocks, 'b')}
</div>
