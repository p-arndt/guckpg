-- Indexed first page; compare with a deep OFFSET to see its cost.
SELECT * FROM public.orders ORDER BY placed_at DESC, id DESC LIMIT 100;
SELECT * FROM public.orders ORDER BY placed_at DESC, id DESC LIMIT 100 OFFSET 900000;

-- Sorting a million rows by an unindexed text expression.
SELECT id, reference, note FROM public.orders ORDER BY note NULLS LAST, id LIMIT 100;

-- Join and aggregate three million order items.
SELECT p.sku, p.name, sum(i.quantity) AS units, sum(i.quantity * i.unit_price) AS revenue
FROM public.order_items i JOIN public.products p ON p.id = i.product_id
GROUP BY p.id ORDER BY revenue DESC LIMIT 20;

-- Compare the live view against its precomputed monthly summary.
SELECT date_trunc('month', placed_at) AS month, count(*), sum(total)
FROM public.order_totals WHERE status IN ('paid', 'shipped') GROUP BY 1 ORDER BY 1;
SELECT * FROM public.monthly_sales ORDER BY month;
