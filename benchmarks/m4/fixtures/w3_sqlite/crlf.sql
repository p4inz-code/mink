CREATE TABLE notes (id INT, body STR, weight INT, tag STR)
INSERT INTO notes VALUES (1, 'alpha', 10, 'x')
INSERT INTO notes VALUES (2, 'beta', 20, 'y')
INSERT INTO notes VALUES (3, 'gamma', 30, 'x')
SELECT COUNT(*) FROM notes
SELECT * FROM notes WHERE tag = 'x'
UPDATE notes SET weight = 99 WHERE id = 2
SELECT * FROM notes WHERE id = 2
DELETE FROM notes WHERE id = 3
SELECT COUNT(*) FROM notes
BEGIN
INSERT INTO notes VALUES (4, 'delta', 40, 'z')
COMMIT
SELECT COUNT(*) FROM notes
