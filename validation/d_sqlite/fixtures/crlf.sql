-- schema
CREATE TABLE items (sku INT, name STR, qty INT)
INSERT INTO items VALUES (1001, 'bolt', 240)
INSERT INTO items VALUES (1002, 'nut', 12)
INSERT INTO items (sku, name) VALUES (1003, 'washer')
-- queries
SELECT * FROM items
SELECT COUNT(*) FROM items
SELECT COUNT(*) FROM items WHERE qty = 12
SELECT * FROM items WHERE qty < 50
SELECT * FROM items WHERE name = 'nut'
-- update then delete
UPDATE items SET qty = 0 WHERE sku = 1001
SELECT * FROM items WHERE qty = 0
DELETE FROM items WHERE sku = 1002
SELECT COUNT(*) FROM items
-- a committed transaction survives
BEGIN
INSERT INTO items VALUES (1004, 'screw', 80)
COMMIT
SELECT COUNT(*) FROM items
-- a rolled-back transaction leaves no trace
BEGIN
INSERT INTO items VALUES (1005, 'ghost', 5)
ROLLBACK
SELECT COUNT(*) FROM items
-- the first error stops the run
