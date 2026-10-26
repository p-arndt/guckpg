\set ON_ERROR_STOP on
BEGIN;
SET LOCAL search_path TO public, pg_catalog;
DO $$
DECLARE
    target_id integer;
    previous_events bigint;
BEGIN
    ASSERT (SELECT count(*) FROM customers) >= 40, 'Missing customers';
    ASSERT (SELECT count(*) FROM orders) >= 180, 'Missing orders';
    ASSERT (SELECT count(*) FROM order_items) >= 480, 'Missing order items';
    ASSERT (SELECT count(*) FROM monthly_sales) >= 6, 'Missing sales summary';
    ASSERT customer_label('customer1@example.test') =
        customer_label((SELECT id FROM customers WHERE email = 'customer1@example.test')),
        'Function overloads disagree';
    SELECT id INTO STRICT target_id FROM orders WHERE status = 'pending' ORDER BY id LIMIT 1;
    SELECT count(*) INTO previous_events FROM audit_log;
    CALL mark_order_paid(target_id);
    ASSERT (SELECT status FROM orders WHERE id = target_id) = 'paid', 'Procedure did not update order';
    ASSERT (SELECT count(*) FROM audit_log) = previous_events + 1, 'Trigger did not log status change';
    ASSERT (SELECT new_status FROM audit_log WHERE order_id = target_id ORDER BY id DESC LIMIT 1) = 'paid',
        'Incorrect audit event';
    REFRESH MATERIALIZED VIEW monthly_sales;
    ASSERT (SELECT sum(order_count) FROM monthly_sales) =
        (SELECT count(*) FROM orders WHERE status IN ('paid', 'shipped')), 'Stale materialized view';
END
$$;
ROLLBACK;
