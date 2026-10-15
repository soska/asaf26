-- An OpenLP Bible database (bibles schema 2), as SQLAlchemy creates it from
-- openlp/plugins/bibles/lib/db.py. King James Version excerpt (public domain),
-- including one deuterocanonical book (book_reference_id 67, Judith).
CREATE TABLE metadata (key VARCHAR(255) NOT NULL, value VARCHAR(255), PRIMARY KEY (key));
CREATE INDEX ix_metadata_key ON metadata (key);
CREATE TABLE book (
    id INTEGER NOT NULL, book_reference_id INTEGER, testament_reference_id INTEGER, name VARCHAR(50),
    PRIMARY KEY (id)
);
CREATE INDEX ix_book_book_reference_id ON book (book_reference_id);
CREATE INDEX ix_book_name ON book (name);
CREATE TABLE verse (
    id INTEGER NOT NULL, book_id INTEGER, chapter INTEGER, verse INTEGER, text TEXT,
    PRIMARY KEY (id), FOREIGN KEY(book_id) REFERENCES book (id)
);
CREATE INDEX ix_verse_book_id ON verse (book_id);
CREATE INDEX ix_verse_chapter ON verse (chapter);
CREATE INDEX ix_verse_verse ON verse (verse);
CREATE INDEX ix_verse_text ON verse (text);
INSERT INTO metadata VALUES ('version', '2');
INSERT INTO metadata VALUES ('name', 'King James Version');
INSERT INTO metadata VALUES ('copyright', 'Public Domain');
INSERT INTO metadata VALUES ('permissions', '');
INSERT INTO metadata VALUES ('language_id', '30');
INSERT INTO metadata VALUES ('book_name_language', '0');
INSERT INTO book VALUES (1, 1, 1, 'Genesis');
INSERT INTO book VALUES (2, 19, 1, 'Psalms');
INSERT INTO book VALUES (3, 43, 2, 'John');
INSERT INTO book VALUES (4, 67, 3, 'Judith');
INSERT INTO verse VALUES (1, 1, 1, 1, 'In the beginning God created the heaven and the earth.');
INSERT INTO verse VALUES (2, 2, 23, 1, 'The LORD is my shepherd; I shall not want.');
INSERT INTO verse VALUES (3, 2, 23, 2, 'He maketh me to lie down in green pastures: he leadeth me beside the still waters.');
INSERT INTO verse VALUES (4, 3, 3, 16, 'For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.');
INSERT INTO verse VALUES (5, 3, 3, 17, '  For God sent not his Son into the world to condemn the world;
but that the world through him might be saved. ');
INSERT INTO verse VALUES (6, 4, 1, 1, 'In the twelfth year of the reign of Nabuchodonosor, who reigned in Nineve, the great city;');
