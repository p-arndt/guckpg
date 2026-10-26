\set ON_ERROR_STOP on
\timing on
-- Run demo.sql first; existing rows and the large performance dataset are preserved.
BEGIN;
SET LOCAL search_path TO public, pg_catalog;

CREATE TABLE IF NOT EXISTS countries (code text PRIMARY KEY, name text NOT NULL);
CREATE TABLE IF NOT EXISTS customer_addresses (
    id integer PRIMARY KEY, customer_id integer NOT NULL REFERENCES customers(id),
    country_code text NOT NULL REFERENCES countries(code), city text NOT NULL,
    postal_code text NOT NULL, street text NOT NULL, kind text NOT NULL
);
CREATE TABLE IF NOT EXISTS categories (
    id integer PRIMARY KEY, parent_id integer REFERENCES categories(id), name text NOT NULL
);
CREATE TABLE IF NOT EXISTS brands (id integer PRIMARY KEY, name text NOT NULL);
CREATE TABLE IF NOT EXISTS product_details (
    product_id integer PRIMARY KEY REFERENCES products(id), brand_id integer REFERENCES brands(id),
    weight_grams integer NOT NULL, dimensions jsonb NOT NULL
);
CREATE TABLE IF NOT EXISTS product_categories (
    product_id integer REFERENCES products(id), category_id integer REFERENCES categories(id),
    PRIMARY KEY (product_id, category_id)
);
CREATE TABLE IF NOT EXISTS suppliers (
    id integer PRIMARY KEY, name text NOT NULL, country_code text REFERENCES countries(code)
);
CREATE TABLE IF NOT EXISTS supplier_products (
    supplier_id integer REFERENCES suppliers(id), product_id integer REFERENCES products(id),
    cost numeric(10,2) NOT NULL, lead_days integer NOT NULL,
    PRIMARY KEY (supplier_id, product_id)
);
CREATE TABLE IF NOT EXISTS warehouses (
    id integer PRIMARY KEY, name text NOT NULL, country_code text REFERENCES countries(code)
);
CREATE TABLE IF NOT EXISTS warehouse_locations (
    warehouse_id integer REFERENCES warehouses(id), code text NOT NULL, capacity integer NOT NULL,
    PRIMARY KEY (warehouse_id, code)
);
CREATE TABLE IF NOT EXISTS inventory (
    warehouse_id integer NOT NULL, location_code text NOT NULL,
    product_id integer REFERENCES products(id), quantity integer NOT NULL CHECK (quantity >= 0),
    PRIMARY KEY (warehouse_id, location_code, product_id),
    FOREIGN KEY (warehouse_id, location_code) REFERENCES warehouse_locations(warehouse_id, code)
);
CREATE TABLE IF NOT EXISTS departments (
    id integer PRIMARY KEY, name text NOT NULL
);
CREATE TABLE IF NOT EXISTS employees (
    id integer PRIMARY KEY, department_id integer REFERENCES departments(id),
    manager_id integer REFERENCES employees(id), name text NOT NULL
);
CREATE TABLE IF NOT EXISTS stock_movements (
    id integer PRIMARY KEY, warehouse_id integer NOT NULL, location_code text NOT NULL,
    product_id integer NOT NULL, employee_id integer REFERENCES employees(id),
    delta integer NOT NULL, moved_at timestamptz NOT NULL,
    FOREIGN KEY (warehouse_id, location_code, product_id)
        REFERENCES inventory(warehouse_id, location_code, product_id)
);
CREATE TABLE IF NOT EXISTS purchase_orders (
    id integer PRIMARY KEY, supplier_id integer REFERENCES suppliers(id),
    warehouse_id integer REFERENCES warehouses(id), buyer_id integer REFERENCES employees(id),
    status text NOT NULL, ordered_at date NOT NULL
);
CREATE TABLE IF NOT EXISTS purchase_order_items (
    purchase_order_id integer REFERENCES purchase_orders(id), product_id integer REFERENCES products(id),
    quantity integer NOT NULL, unit_cost numeric(10,2) NOT NULL,
    PRIMARY KEY (purchase_order_id, product_id)
);
CREATE TABLE IF NOT EXISTS carriers (id integer PRIMARY KEY, name text NOT NULL);
CREATE TABLE IF NOT EXISTS shipments (
    id integer PRIMARY KEY, order_id integer NOT NULL REFERENCES orders(id),
    warehouse_id integer REFERENCES warehouses(id), carrier_id integer REFERENCES carriers(id),
    tracking_number text UNIQUE NOT NULL, shipped_at timestamptz
);
CREATE TABLE IF NOT EXISTS shipment_items (
    shipment_id integer REFERENCES shipments(id), order_id integer NOT NULL, product_id integer NOT NULL,
    quantity integer NOT NULL, PRIMARY KEY (shipment_id, product_id),
    FOREIGN KEY (order_id, product_id) REFERENCES order_items(order_id, product_id)
);
CREATE TABLE IF NOT EXISTS shipment_events (
    id integer PRIMARY KEY, shipment_id integer REFERENCES shipments(id),
    status text NOT NULL, occurred_at timestamptz NOT NULL
);
CREATE TABLE IF NOT EXISTS invoices (
    id integer PRIMARY KEY, order_id integer NOT NULL REFERENCES orders(id),
    number text UNIQUE NOT NULL, issued_at date NOT NULL, due_at date NOT NULL
);
CREATE TABLE IF NOT EXISTS invoice_items (
    invoice_id integer REFERENCES invoices(id), product_id integer REFERENCES products(id),
    quantity integer NOT NULL, unit_price numeric(10,2) NOT NULL,
    PRIMARY KEY (invoice_id, product_id)
);
CREATE TABLE IF NOT EXISTS payment_methods (id integer PRIMARY KEY, name text NOT NULL);
CREATE TABLE IF NOT EXISTS payments (
    id integer PRIMARY KEY, invoice_id integer REFERENCES invoices(id),
    method_id integer REFERENCES payment_methods(id), amount numeric(12,2) NOT NULL,
    paid_at timestamptz NOT NULL
);
CREATE TABLE IF NOT EXISTS returns (
    id integer PRIMARY KEY, order_id integer REFERENCES orders(id),
    handled_by integer REFERENCES employees(id), reason text NOT NULL, status text NOT NULL
);
CREATE TABLE IF NOT EXISTS return_items (
    return_id integer REFERENCES returns(id), order_id integer NOT NULL, product_id integer NOT NULL,
    quantity integer NOT NULL, PRIMARY KEY (return_id, product_id),
    FOREIGN KEY (order_id, product_id) REFERENCES order_items(order_id, product_id)
);
CREATE TABLE IF NOT EXISTS refunds (
    id integer PRIMARY KEY, payment_id integer REFERENCES payments(id),
    return_id integer REFERENCES returns(id), amount numeric(12,2) NOT NULL, refunded_at timestamptz NOT NULL
);
CREATE TABLE IF NOT EXISTS coupons (
    id integer PRIMARY KEY, code text UNIQUE NOT NULL, discount_percent integer NOT NULL CHECK (discount_percent BETWEEN 1 AND 100)
);
CREATE TABLE IF NOT EXISTS order_coupons (
    order_id integer REFERENCES orders(id), coupon_id integer REFERENCES coupons(id),
    PRIMARY KEY (order_id, coupon_id)
);
CREATE TABLE IF NOT EXISTS reviews (
    id integer PRIMARY KEY, customer_id integer REFERENCES customers(id), product_id integer REFERENCES products(id),
    rating integer NOT NULL CHECK (rating BETWEEN 1 AND 5), body text NOT NULL
);
CREATE TABLE IF NOT EXISTS support_tickets (
    id integer PRIMARY KEY, customer_id integer REFERENCES customers(id), order_id integer REFERENCES orders(id),
    assigned_to integer REFERENCES employees(id), subject text NOT NULL, status text NOT NULL
);
CREATE TABLE IF NOT EXISTS ticket_messages (
    id integer PRIMARY KEY, ticket_id integer REFERENCES support_tickets(id),
    employee_id integer REFERENCES employees(id), body text NOT NULL, sent_at timestamptz NOT NULL
);

-- Map actual keys so identity gaps and repeated seeds do not break relationships.
CREATE TEMP TABLE seed_products ON COMMIT DROP AS
SELECT row_number() OVER (ORDER BY id)::integer AS n, id, price FROM products ORDER BY id LIMIT 2000;
CREATE TEMP TABLE seed_orders ON COMMIT DROP AS
SELECT row_number() OVER (ORDER BY id)::integer AS n, id, customer_id, placed_at
FROM orders WHERE EXISTS (SELECT 1 FROM order_items i WHERE i.order_id = orders.id)
ORDER BY id LIMIT 10000;

INSERT INTO countries VALUES ('DE', 'Germany'), ('AT', 'Austria'), ('CH', 'Switzerland'), ('NL', 'Netherlands') ON CONFLICT DO NOTHING;
INSERT INTO customer_addresses
SELECT n, customer_id, 'DE', 'Berlin', '10115', 'Example Street ' || n, 'shipping'
FROM seed_orders ON CONFLICT DO NOTHING;
INSERT INTO categories SELECT n, CASE WHEN n > 5 THEN 1 + n % 5 END, 'Category ' || n FROM generate_series(1, 40) n ON CONFLICT DO NOTHING;
INSERT INTO brands SELECT n, 'Brand ' || n FROM generate_series(1, 30) n ON CONFLICT DO NOTHING;
INSERT INTO product_details SELECT id, 1 + n % 30, 100 + n % 5000, jsonb_build_object('width', 10 + n % 50, 'height', 5 + n % 20) FROM seed_products ON CONFLICT DO NOTHING;
INSERT INTO product_categories SELECT id, 1 + n % 40 FROM seed_products ON CONFLICT DO NOTHING;
INSERT INTO suppliers SELECT n, 'Supplier ' || n, (ARRAY['DE','AT','CH','NL'])[1 + n % 4] FROM generate_series(1, 50) n ON CONFLICT DO NOTHING;
INSERT INTO supplier_products SELECT 1 + n % 50, id, round(price * 0.6, 2), 1 + n % 21 FROM seed_products ON CONFLICT DO NOTHING;
INSERT INTO warehouses SELECT n, 'Warehouse ' || n, (ARRAY['DE','AT','CH','NL'])[1 + n % 4] FROM generate_series(1, 8) n ON CONFLICT DO NOTHING;
INSERT INTO warehouse_locations SELECT w.id, 'BIN-' || n, 5000 FROM warehouses w CROSS JOIN generate_series(1, 20) n ON CONFLICT DO NOTHING;
INSERT INTO inventory SELECT w.id, 'BIN-' || (1 + p.n % 20), p.id, 10 + p.n % 300 FROM warehouses w CROSS JOIN seed_products p ON CONFLICT DO NOTHING;
INSERT INTO departments SELECT n, (ARRAY['Sales','Logistics','Support','Purchasing','Finance'])[n] FROM generate_series(1, 5) n ON CONFLICT DO NOTHING;
INSERT INTO employees SELECT n, 1 + n % 5, CASE WHEN n > 5 THEN 1 + n % 5 END, 'Employee ' || n FROM generate_series(1, 100) n ON CONFLICT DO NOTHING;
INSERT INTO stock_movements SELECT p.n, 1, 'BIN-' || (1 + p.n % 20), p.id, 1 + p.n % 100, 5, TIMESTAMPTZ '2026-01-01' + p.n * INTERVAL '1 minute' FROM seed_products p ON CONFLICT DO NOTHING;
INSERT INTO purchase_orders SELECT n, 1 + n % 50, 1 + n % 8, 1 + n % 100, 'received', DATE '2025-01-01' + n % 365 FROM generate_series(1, 1000) n ON CONFLICT DO NOTHING;
INSERT INTO purchase_order_items SELECT 1 + n % 1000, id, 10 + n % 100, round(price * 0.6, 2) FROM seed_products ON CONFLICT DO NOTHING;
INSERT INTO carriers VALUES (1,'Parcel Express'), (2,'Demo Logistics'), (3,'Fast Freight') ON CONFLICT DO NOTHING;
INSERT INTO shipments SELECT n, id, 1 + n % 8, 1 + n % 3, 'TRACK-' || n, placed_at + INTERVAL '1 day' FROM seed_orders ON CONFLICT DO NOTHING;
INSERT INTO shipment_items SELECT o.n, i.order_id, i.product_id, i.quantity FROM seed_orders o JOIN order_items i ON i.order_id = o.id ON CONFLICT DO NOTHING;
INSERT INTO shipment_events SELECT o.n * 10 + e, o.n, (ARRAY['picked','in_transit','delivered'])[e], o.placed_at + e * INTERVAL '1 day' FROM seed_orders o CROSS JOIN generate_series(1, 3) e ON CONFLICT DO NOTHING;
INSERT INTO invoices SELECT n, id, 'INV-' || n, placed_at::date, placed_at::date + 14 FROM seed_orders ON CONFLICT DO NOTHING;
INSERT INTO invoice_items SELECT o.n, i.product_id, i.quantity, i.unit_price FROM seed_orders o JOIN order_items i ON i.order_id = o.id ON CONFLICT DO NOTHING;
INSERT INTO payment_methods VALUES (1,'Card'), (2,'Bank transfer'), (3,'Wallet') ON CONFLICT DO NOTHING;
INSERT INTO payments SELECT o.n, o.n, 1 + o.n % 3, sum(i.quantity * i.unit_price), o.placed_at + INTERVAL '2 days' FROM seed_orders o JOIN order_items i ON i.order_id = o.id GROUP BY o.n, o.placed_at ON CONFLICT DO NOTHING;
INSERT INTO returns SELECT n, id, 1 + n % 100, 'Does not fit', 'refunded' FROM seed_orders WHERE n % 20 = 0 ON CONFLICT DO NOTHING;
INSERT INTO return_items SELECT o.n, i.order_id, i.product_id, i.quantity FROM seed_orders o JOIN order_items i ON i.order_id = o.id WHERE o.n % 20 = 0 ON CONFLICT DO NOTHING;
INSERT INTO refunds SELECT p.id, p.id, r.id, p.amount, p.paid_at + INTERVAL '7 days' FROM payments p JOIN returns r ON r.id = p.id ON CONFLICT DO NOTHING;
INSERT INTO coupons SELECT n, 'SAVE-' || n, 5 + n % 20 FROM generate_series(1, 50) n ON CONFLICT DO NOTHING;
INSERT INTO order_coupons SELECT id, 1 + n % 50 FROM seed_orders WHERE n % 4 = 0 ON CONFLICT DO NOTHING;
INSERT INTO reviews SELECT o.n, o.customer_id, min(i.product_id), 1 + o.n % 5, 'Example review for order ' || o.n FROM seed_orders o JOIN order_items i ON i.order_id = o.id GROUP BY o.n, o.customer_id ON CONFLICT DO NOTHING;
INSERT INTO support_tickets SELECT n, customer_id, id, 1 + n % 100, 'Question about order ' || n, CASE WHEN n % 3 = 0 THEN 'open' ELSE 'closed' END FROM seed_orders WHERE n % 5 = 0 ON CONFLICT DO NOTHING;
INSERT INTO ticket_messages SELECT t.id * 10 + n, t.id, CASE WHEN n > 1 THEN t.assigned_to END, 'Example message ' || n, TIMESTAMPTZ '2026-01-01' + n * INTERVAL '1 hour' FROM support_tickets t CROSS JOIN generate_series(1, 3) n ON CONFLICT DO NOTHING;

CREATE OR REPLACE VIEW warehouse_stock AS
SELECT w.id, w.name, count(DISTINCT i.product_id) AS products, sum(i.quantity) AS units
FROM warehouses w JOIN inventory i ON i.warehouse_id = w.id GROUP BY w.id;
CREATE OR REPLACE VIEW open_support_tickets AS
SELECT t.id, t.subject, c.name AS customer, e.name AS assignee, o.reference
FROM support_tickets t JOIN customers c ON c.id = t.customer_id
LEFT JOIN employees e ON e.id = t.assigned_to LEFT JOIN orders o ON o.id = t.order_id
WHERE t.status = 'open';
COMMIT;
