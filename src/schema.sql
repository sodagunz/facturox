CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 200),
    barcode TEXT UNIQUE CHECK(barcode IS NULL OR length(barcode) BETWEEN 1 AND 100),
    price_cents INTEGER NOT NULL CHECK(price_cents >= 0)
);
CREATE TABLE IF NOT EXISTS stock_movements (
    id INTEGER PRIMARY KEY,
    product_id INTEGER NOT NULL REFERENCES products(id),
    delta INTEGER NOT NULL CHECK(delta <> 0),
    reason TEXT NOT NULL CHECK(length(trim(reason)) > 0),
    order_id INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS stock_movements_product ON stock_movements(product_id);
CREATE TABLE IF NOT EXISTS orders (
    id INTEGER PRIMARY KEY,
    status TEXT NOT NULL CHECK(status IN ('open', 'closed', 'cancelled')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ended_at TEXT,
    CHECK ((status = 'open' AND ended_at IS NULL) OR (status <> 'open' AND ended_at IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS orders_one_open ON orders(status) WHERE status = 'open';
CREATE TABLE IF NOT EXISTS order_lines (
    order_id INTEGER NOT NULL REFERENCES orders(id),
    product_id INTEGER NOT NULL REFERENCES products(id),
    name TEXT NOT NULL,
    unit_price_cents INTEGER NOT NULL CHECK(unit_price_cents >= 0),
    quantity INTEGER NOT NULL CHECK(quantity > 0),
    PRIMARY KEY (order_id, product_id)
);
CREATE TABLE IF NOT EXISTS order_commands (
    request_id TEXT PRIMARY KEY CHECK(length(request_id) BETWEEN 1 AND 128),
    kind TEXT NOT NULL CHECK(kind IN ('add', 'remove', 'close', 'cancel')),
    order_id INTEGER NOT NULL REFERENCES orders(id),
    product_id INTEGER,
    quantity INTEGER,
    CHECK ((kind IN ('add', 'remove') AND product_id IS NOT NULL AND quantity > 0) OR
           (kind IN ('close', 'cancel') AND product_id IS NULL AND quantity IS NULL))
);
CREATE TABLE IF NOT EXISTS stock_adjustment_commands (
    request_id TEXT PRIMARY KEY NOT NULL,
    product_id INTEGER NOT NULL REFERENCES products(id),
    delta INTEGER NOT NULL CHECK(delta <> 0),
    reason TEXT NOT NULL CHECK(length(trim(reason)) > 0),
    result_quantity INTEGER NOT NULL CHECK(result_quantity >= 0)
);
CREATE TABLE IF NOT EXISTS app_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    business_name TEXT NOT NULL DEFAULT '',
    tax_identifier TEXT NOT NULL DEFAULT '',
    address TEXT NOT NULL DEFAULT '',
    contact TEXT NOT NULL DEFAULT '',
    theme TEXT NOT NULL DEFAULT 'light' CHECK (theme IN ('light', 'dark'))
);
INSERT INTO app_settings (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
