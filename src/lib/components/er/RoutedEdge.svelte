<script lang="ts">
	import { BaseEdge, getSmoothStepPath, type EdgeProps } from '@xyflow/svelte';
	import { midpoint, roundedPath, snapEnds, type Point } from '#lib/er-route.js';

	let {
		sourceX,
		sourceY,
		targetX,
		targetY,
		sourcePosition,
		targetPosition,
		data,
		label,
		labelStyle,
		style,
		markerEnd,
		interactionWidth
	}: EdgeProps = $props();

	// The routed line when it still fits the handles, a simple stepped line otherwise.
	const drawn = $derived.by(() => {
		const route = data?.points as Point[] | undefined;
		const points = route
			? snapEnds(route, { x: sourceX, y: sourceY }, { x: targetX, y: targetY })
			: null;
		if (points) {
			const mid = midpoint(points);
			return { path: roundedPath(points), labelX: mid.x, labelY: mid.y };
		}
		const [path, labelX, labelY] = getSmoothStepPath({
			sourceX,
			sourceY,
			targetX,
			targetY,
			sourcePosition,
			targetPosition,
			borderRadius: 8,
			offset: 20
		});
		return { path, labelX, labelY };
	});
</script>

<BaseEdge
	path={drawn.path}
	labelX={drawn.labelX}
	labelY={drawn.labelY}
	{label}
	{labelStyle}
	{style}
	{markerEnd}
	{interactionWidth}
/>
