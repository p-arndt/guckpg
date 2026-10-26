\set ON_ERROR_STOP on
BEGIN;
DO $$
DECLARE
    table_name text;
    row_count bigint;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'countries', 'customer_addresses', 'categories', 'brands', 'product_details',
        'product_categories', 'suppliers', 'supplier_products', 'warehouses',
        'warehouse_locations', 'inventory', 'departments', 'employees', 'stock_movements',
        'purchase_orders', 'purchase_order_items', 'carriers', 'shipments', 'shipment_items',
        'shipment_events', 'invoices', 'invoice_items', 'payment_methods', 'payments',
        'returns', 'return_items', 'refunds', 'coupons', 'order_coupons', 'reviews',
        'support_tickets', 'ticket_messages'
    ] LOOP
        EXECUTE format('SELECT count(*) FROM public.%I', table_name) INTO row_count;
        ASSERT row_count > 0, format('Empty demo table: %s', table_name);
    END LOOP;
    ASSERT (SELECT count(*) FROM public.shipments) = 10000;
    ASSERT (SELECT count(*) FROM public.shipment_events) = 30000;
    ASSERT NOT EXISTS (
        SELECT 1 FROM public.shipment_items i JOIN public.shipments s ON s.id = i.shipment_id
        WHERE i.order_id <> s.order_id
    ), 'Shipment belongs to a different order';
    ASSERT NOT EXISTS (
        SELECT 1 FROM public.payments p JOIN public.invoice_items i ON i.invoice_id = p.invoice_id
        GROUP BY p.id HAVING p.amount <> sum(i.quantity * i.unit_price)
    ), 'Payment does not match invoice total';
    ASSERT NOT EXISTS (
        SELECT 1 FROM public.refunds r JOIN public.payments p ON p.id = r.payment_id
        JOIN public.invoices i ON i.id = p.invoice_id JOIN public.returns t ON t.id = r.return_id
        WHERE r.amount > p.amount OR i.order_id <> t.order_id
    ), 'Refund does not match returned order';
    ASSERT (SELECT count(*) FROM public.warehouse_stock) = 8;
    ASSERT (SELECT count(*) FROM public.open_support_tickets) > 0;
END
$$;
ROLLBACK;

SELECT c.relkind, count(*) AS objects
FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'public' AND c.relkind IN ('r', 'v', 'm') GROUP BY c.relkind;
SELECT count(*) AS foreign_keys FROM pg_constraint c
JOIN pg_namespace n ON n.oid = c.connamespace WHERE n.nspname = 'public' AND c.contype = 'f';
