\set ON_ERROR_STOP on

BEGIN;
SET LOCAL search_path TO public, pg_catalog;

CREATE TABLE IF NOT EXISTS customers (
    id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL,
    email text UNIQUE NOT NULL,
    active boolean NOT NULL DEFAULT true,
    birthday date,
    preferences jsonb NOT NULL DEFAULT '{}',
    tags text[] NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS products (
    id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sku text UNIQUE NOT NULL,
    name text NOT NULL,
    price numeric(10,2) NOT NULL CHECK (price >= 0),
    stock integer NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS orders (
    id integer GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    reference text UNIQUE NOT NULL,
    customer_id integer NOT NULL REFERENCES customers(id),
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'paid', 'shipped', 'cancelled')),
    placed_at timestamptz NOT NULL DEFAULT now(),
    note text
);
CREATE TABLE IF NOT EXISTS order_items (
    order_id integer REFERENCES orders(id),
    product_id integer REFERENCES products(id),
    quantity integer NOT NULL CHECK (quantity > 0),
    unit_price numeric(10,2) NOT NULL,
    PRIMARY KEY (order_id, product_id)
);
CREATE TABLE IF NOT EXISTS audit_log (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    order_id integer NOT NULL,
    old_status text,
    new_status text NOT NULL,
    changed_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS type_samples (
    id integer PRIMARY KEY,
    uuid_value uuid,
    binary_value bytea,
    duration interval,
    ip_address inet,
    location point,
    date_range daterange,
    special_text text,
    nullable_text text
);
CREATE INDEX IF NOT EXISTS orders_customer_idx ON orders(customer_id);
CREATE SEQUENCE IF NOT EXISTS invoice_number START WITH 10000 INCREMENT BY 10;

INSERT INTO customers (name, email, active, birthday, preferences, tags)
SELECT 'Customer ' || n, 'customer' || n || '@example.test', n % 7 <> 0,
       DATE '1980-01-01' + n * 137,
       jsonb_build_object('language', CASE WHEN n % 2 = 0 THEN 'de' ELSE 'en' END, 'newsletter', n % 3 = 0),
       CASE WHEN n % 4 = 0 THEN ARRAY['vip', 'online'] ELSE ARRAY['online'] END
FROM generate_series(1, 40) n
ON CONFLICT (email) DO NOTHING;

INSERT INTO products (sku, name, price, stock)
SELECT 'DEMO-' || lpad(n::text, 3, '0'),
       (ARRAY['Keyboard', 'Mouse', 'Monitor', 'Desk lamp', 'USB hub', 'Headphones', 'Webcam', 'Notebook'])[n],
       (ARRAY[89.90, 29.50, 349.00, 45.00, 39.90, 119.00, 79.00, 4.50])[n], n * 12
FROM generate_series(1, 8) n
ON CONFLICT (sku) DO NOTHING;

INSERT INTO orders (reference, customer_id, status, placed_at, note)
SELECT 'DEMO-ORDER-' || n, c.id,
       (ARRAY['pending', 'paid', 'shipped', 'cancelled'])[1 + n % 4],
       TIMESTAMPTZ '2026-01-01 10:00:00+00' + n * INTERVAL '1 day',
       CASE WHEN n % 5 = 0 THEN E'Please ring twice.\nLeave at reception.' END
FROM generate_series(1, 180) n
JOIN customers c ON c.email = 'customer' || (1 + n % 40) || '@example.test'
ON CONFLICT (reference) DO NOTHING;

INSERT INTO order_items (order_id, product_id, quantity, unit_price)
SELECT o.id, p.id, 1 + p.id % 3, p.price
FROM orders o CROSS JOIN products p
WHERE o.reference LIKE 'DEMO-ORDER-%' AND p.sku LIKE 'DEMO-%' AND (o.id + p.id) % 3 = 0
ON CONFLICT (order_id, product_id) DO NOTHING;

INSERT INTO type_samples VALUES
    (1, '12345678-1234-4567-8901-123456789abc', decode('48656c6c6f', 'hex'),
     INTERVAL '2 days 03:04:05', '192.0.2.42', point(13.4, 52.5),
     '[2026-01-01,2026-02-01)', E'Umlauts: äöü ß — emoji: 🚀\nQuotes: ''hello''', NULL),
    (2, NULL, ''::bytea, INTERVAL '0 seconds', '2001:db8::1', point(0, 0),
     'empty', '', 'Not NULL')
ON CONFLICT (id) DO NOTHING;

CREATE OR REPLACE VIEW order_totals AS
SELECT o.id, o.reference, c.name AS customer, o.status, o.placed_at,
       coalesce(sum(i.quantity * i.unit_price), 0)::numeric(12,2) AS total
FROM orders o JOIN customers c ON c.id = o.customer_id
LEFT JOIN order_items i ON i.order_id = o.id
GROUP BY o.id, c.name;

CREATE OR REPLACE VIEW active_customers WITH (security_barrier = true) AS
SELECT id, name, email FROM customers WHERE active;

CREATE MATERIALIZED VIEW IF NOT EXISTS monthly_sales AS
SELECT date_trunc('month', placed_at) AS month, count(*) AS order_count,
       sum(total) AS revenue
FROM order_totals WHERE status IN ('paid', 'shipped') GROUP BY 1;
CREATE UNIQUE INDEX IF NOT EXISTS monthly_sales_month_idx ON monthly_sales(month);
REFRESH MATERIALIZED VIEW monthly_sales;

CREATE OR REPLACE FUNCTION customer_label(customer_id integer) RETURNS text
LANGUAGE sql STABLE SET search_path = public, pg_catalog AS $$
    SELECT name || ' <' || email || '>' FROM customers WHERE id = customer_id
$$;
CREATE OR REPLACE FUNCTION customer_label(customer_email text) RETURNS text
LANGUAGE sql STABLE SET search_path = public, pg_catalog AS $$
    SELECT name || ' <' || email || '>' FROM customers WHERE email = customer_email
$$;
CREATE OR REPLACE FUNCTION log_order_status() RETURNS trigger
LANGUAGE plpgsql SET search_path = public, pg_catalog AS $$
BEGIN
    IF OLD.status IS DISTINCT FROM NEW.status THEN
        INSERT INTO audit_log (order_id, old_status, new_status)
        VALUES (NEW.id, OLD.status, NEW.status);
    END IF;
    RETURN NEW;
END
$$;
CREATE OR REPLACE TRIGGER order_status_changed AFTER UPDATE OF status ON orders
FOR EACH ROW EXECUTE FUNCTION log_order_status();

CREATE OR REPLACE PROCEDURE mark_order_paid(target_order_id integer)
LANGUAGE sql SET search_path = public, pg_catalog AS $$
    UPDATE orders SET status = 'paid' WHERE id = target_order_id AND status = 'pending'
$$;
COMMIT;
