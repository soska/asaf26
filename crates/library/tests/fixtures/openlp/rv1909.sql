-- An OpenLP Bible database: Reina-Valera 1909 excerpt (public domain). No
-- language_id, so the language comes from the book names.
CREATE TABLE metadata (key VARCHAR(255) NOT NULL, value VARCHAR(255), PRIMARY KEY (key));
CREATE TABLE book (
    id INTEGER NOT NULL, book_reference_id INTEGER, testament_reference_id INTEGER, name VARCHAR(50),
    PRIMARY KEY (id)
);
CREATE TABLE verse (
    id INTEGER NOT NULL, book_id INTEGER, chapter INTEGER, verse INTEGER, text TEXT,
    PRIMARY KEY (id), FOREIGN KEY(book_id) REFERENCES book (id)
);
INSERT INTO metadata VALUES ('name', 'Reina-Valera 1909');
INSERT INTO metadata VALUES ('copyright', 'Dominio público');
INSERT INTO book VALUES (1, 1, 1, 'Génesis');
INSERT INTO book VALUES (2, 19, 1, 'Salmos');
INSERT INTO book VALUES (3, 43, 2, 'San Juan');
INSERT INTO verse VALUES (1, 1, 1, 1, 'EN el principio crió Dios los cielos y la tierra.');
INSERT INTO verse VALUES (2, 2, 23, 1, 'Salmo de David. JEHOVÁ es mi pastor; nada me faltará.');
INSERT INTO verse VALUES (3, 3, 3, 16, 'Porque de tal manera amó Dios al mundo, que ha dado á su Hijo unigénito, para que todo aquel que en él cree, no se pierda, mas tenga vida eterna.');
INSERT INTO verse VALUES (4, 3, 3, 17, 'Porque no envió Dios á su Hijo al mundo para que condene al mundo, mas para que el mundo sea salvo por él.');
