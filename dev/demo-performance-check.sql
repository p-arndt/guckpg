\set ON_ERROR_STOP on
BEGIN;
DO $$
BEGIN
    ASSERT (SELECT count(*) FROM public.customers WHERE email LIKE 'perf%@example.test') = 100000;
    ASSERT (SELECT count(*) FROM public.products WHERE sku LIKE 'PERF-%') = 10000;
    ASSERT (SELECT count(*) FROM public.orders WHERE reference LIKE 'PERF-ORDER-%') = 1000000;
    ASSERT (SELECT count(*) FROM public.order_items i JOIN public.orders o ON o.id = i.order_id
            WHERE o.reference LIKE 'PERF-ORDER-%') = 3000000;
    ASSERT NOT EXISTS (
        SELECT o.id FROM public.orders o LEFT JOIN public.order_items i ON i.order_id = o.id
        WHERE o.reference LIKE 'PERF-ORDER-%' GROUP BY o.id HAVING count(*) <> 3
    ), 'Each performance order must have three items';
    ASSERT (SELECT sum(order_count) FROM public.monthly_sales) =
        (SELECT count(*) FROM public.orders WHERE status IN ('paid', 'shipped'));
END
$$;
ROLLBACK;
