-- An OpenLP web Bible: only metadata and the books fetched so far; its text
-- is downloaded on demand, so there's nothing to import.
CREATE TABLE metadata (key VARCHAR(255) NOT NULL, value VARCHAR(255), PRIMARY KEY (key));
CREATE TABLE book (id INTEGER NOT NULL, book_reference_id INTEGER, testament_reference_id INTEGER, name VARCHAR(50), PRIMARY KEY (id));
CREATE TABLE verse (id INTEGER NOT NULL, book_id INTEGER, chapter INTEGER, verse INTEGER, text TEXT, PRIMARY KEY (id));
INSERT INTO metadata VALUES ('name', 'NIV');
INSERT INTO metadata VALUES ('download_source', 'BibleGateway');
INSERT INTO metadata VALUES ('download_name', 'New International Version');
INSERT INTO book VALUES (1, 43, 2, 'John');
INSERT INTO verse VALUES (1, 1, 3, 16, 'Fetched text that a test must not import.');
