\set ON_ERROR_STOP on
\timing on

-- Run demo.sql first. Stable business keys make this additive and rerunnable.
BEGIN;
SET LOCAL search_path TO public, pg_catalog;

INSERT INTO customers (name, email, active, birthday, preferences, tags)
SELECT 'Performance Customer ' || n, 'perf' || n || '@example.test', n % 11 <> 0,
       DATE '1950-01-01' + n % 23000,
       jsonb_build_object('language', CASE WHEN n % 3 = 0 THEN 'de' ELSE 'en' END,
                          'newsletter', n % 5 = 0, 'segment', n % 12),
       CASE WHEN n % 10 = 0 THEN ARRAY['vip', 'performance'] ELSE ARRAY['performance'] END
FROM generate_series(1, 100000) n
WHERE NOT EXISTS (SELECT 1 FROM customers WHERE email = 'perf' || n || '@example.test')
ON CONFLICT (email) DO NOTHING;

INSERT INTO products (sku, name, price, stock)
SELECT 'PERF-' || n, 'Product ' || n || ' — ' ||
       (ARRAY['Office', 'Audio', 'Travel', 'Home'])[1 + n % 4],
       (99 + n % 150000)::numeric / 100, n % 500
FROM generate_series(1, 10000) n
WHERE NOT EXISTS (SELECT 1 FROM products WHERE sku = 'PERF-' || n)
ON CONFLICT (sku) DO NOTHING;

-- A fifth of customers receive most orders, so indexed lookups have varied selectivity.
INSERT INTO orders (reference, customer_id, status, placed_at, note)
SELECT 'PERF-ORDER-' || n, c.id,
       CASE WHEN n % 100 < 60 THEN 'shipped' WHEN n % 100 < 80 THEN 'paid'
            WHEN n % 100 < 95 THEN 'pending' ELSE 'cancelled' END,
       TIMESTAMPTZ '2023-01-01 00:00:00+00' + (n * 7919::bigint % 119000000) * INTERVAL '1 second',
       CASE WHEN n % 4 = 0 THEN repeat('Delivery instructions for performance testing. ', 8)
            WHEN n % 7 = 0 THEN E'Gift order\nDo not include the invoice.' END
FROM generate_series(1, 1000000) n
JOIN customers c ON c.email = 'perf' ||
    (1 + CASE WHEN n % 5 <> 0 THEN n % 20000 ELSE n % 100000 END) || '@example.test'
WHERE NOT EXISTS (SELECT 1 FROM orders WHERE reference = 'PERF-ORDER-' || n)
ON CONFLICT (reference) DO NOTHING;

INSERT INTO order_items (order_id, product_id, quantity, unit_price)
SELECT o.id, p.id, 1 + (n + slot) % 5, p.price
FROM generate_series(1, 1000000) n
JOIN orders o ON o.reference = 'PERF-ORDER-' || n
CROSS JOIN generate_series(1, 3) slot
JOIN products p ON p.sku = 'PERF-' || (1 + (n * 17 + slot * 997) % 10000)
WHERE NOT EXISTS (
    SELECT 1 FROM order_items i WHERE i.order_id = o.id AND i.product_id = p.id
)
ON CONFLICT (order_id, product_id) DO NOTHING;

-- Keep date sorting indexed while name/price sorting remains useful for scan comparisons.
CREATE INDEX IF NOT EXISTS orders_placed_at_idx ON orders (placed_at DESC, id DESC);
ANALYZE customers;
ANALYZE products;
ANALYZE orders;
ANALYZE order_items;
REFRESH MATERIALIZED VIEW monthly_sales;
ANALYZE monthly_sales;
COMMIT;

SELECT 'customers' AS object, count(*) AS rows FROM public.customers
UNION ALL SELECT 'products', count(*) FROM public.products
UNION ALL SELECT 'orders', count(*) FROM public.orders
UNION ALL SELECT 'order_items', count(*) FROM public.order_items;
