CREATE TABLE inventory_events (
 "order" INTEGER PRIMARY KEY CHECK("order" > 0),
 payload BLOB NOT NULL,
 occurred_at INTEGER NOT NULL CHECK(occurred_at >= 0),
 operation_id TEXT NOT NULL REFERENCES inventory_commands(id) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE inventory_commands (
 id TEXT PRIMARY KEY,
 request TEXT NOT NULL,
 receipt TEXT NOT NULL
) STRICT;
CREATE TABLE products (
 id TEXT PRIMARY KEY,
 name TEXT NOT NULL UNIQUE CHECK(length(name) BETWEEN 1 AND 200 AND trim(name)=name),
 barcode TEXT UNIQUE CHECK(barcode IS NULL OR (length(barcode) BETWEEN 1 AND 100 AND trim(barcode)=barcode)),
 price INTEGER NOT NULL CHECK(price >= 0),
 quantity INTEGER NOT NULL CHECK(quantity >= 0)
) STRICT;
CREATE TABLE orders (id TEXT PRIMARY KEY, payload BLOB NOT NULL) STRICT;
CREATE TABLE app_settings (
 id INTEGER PRIMARY KEY CHECK(id=1),
 theme TEXT NOT NULL CHECK(theme IN ('light','dark')),
 business_name TEXT NOT NULL DEFAULT '',
 tax_identifier TEXT NOT NULL DEFAULT '',
 address TEXT NOT NULL DEFAULT '',
 contact TEXT NOT NULL DEFAULT ''
) STRICT;
INSERT INTO app_settings(id,theme) VALUES (1,'light');
