-- An OpenLP 3.x songs database (schema version 9), as SQLAlchemy creates it
-- from openlp/plugins/songs/lib/db.py. Public-domain hymns only.
CREATE TABLE metadata (key VARCHAR(64) NOT NULL, value VARCHAR(255), PRIMARY KEY (key));
INSERT INTO metadata VALUES ('version', '9');
CREATE TABLE authors (
    id INTEGER NOT NULL, first_name VARCHAR(128), last_name VARCHAR(128), display_name VARCHAR(255) NOT NULL,
    PRIMARY KEY (id)
);
CREATE INDEX ix_authors_display_name ON authors (display_name);
CREATE TABLE song_books (id INTEGER NOT NULL, name VARCHAR(128) NOT NULL, publisher VARCHAR(128), PRIMARY KEY (id));
CREATE TABLE songs (
    id INTEGER NOT NULL,
    title VARCHAR(255) NOT NULL,
    alternate_title VARCHAR(255),
    lyrics TEXT NOT NULL,
    verse_order VARCHAR(128),
    copyright VARCHAR(255),
    comments TEXT,
    ccli_number VARCHAR(64),
    theme_name VARCHAR(128),
    search_title VARCHAR(255) NOT NULL,
    search_lyrics TEXT NOT NULL,
    create_date DATETIME,
    last_modified DATETIME,
    temporary BOOLEAN,
    is_favourite BOOLEAN,
    PRIMARY KEY (id)
);
CREATE INDEX ix_songs_search_title ON songs (search_title);
CREATE TABLE topics (id INTEGER NOT NULL, name VARCHAR(128) NOT NULL, PRIMARY KEY (id));
CREATE TABLE authors_songs (
    author_id INTEGER NOT NULL, song_id INTEGER NOT NULL, author_type VARCHAR(255) DEFAULT "" NOT NULL,
    PRIMARY KEY (author_id, song_id, author_type),
    FOREIGN KEY(author_id) REFERENCES authors (id), FOREIGN KEY(song_id) REFERENCES songs (id)
);
CREATE TABLE media_files (
    id INTEGER NOT NULL, song_id INTEGER, file_path VARCHAR NOT NULL, file_hash VARCHAR(128) NOT NULL,
    type VARCHAR(64) NOT NULL, weight INTEGER, PRIMARY KEY (id), FOREIGN KEY(song_id) REFERENCES songs (id)
);
CREATE TABLE songs_songbooks (
    songbook_id INTEGER NOT NULL, song_id INTEGER NOT NULL, entry VARCHAR(255) NOT NULL,
    PRIMARY KEY (songbook_id, song_id, entry),
    FOREIGN KEY(songbook_id) REFERENCES song_books (id), FOREIGN KEY(song_id) REFERENCES songs (id)
);
CREATE TABLE songs_topics (
    song_id INTEGER NOT NULL, topic_id INTEGER NOT NULL, PRIMARY KEY (song_id, topic_id),
    FOREIGN KEY(song_id) REFERENCES songs (id), FOREIGN KEY(topic_id) REFERENCES topics (id)
);

INSERT INTO authors VALUES (1, 'John', 'Newton', 'John Newton');
INSERT INTO authors VALUES (2, 'Reginald', 'Heber', 'Reginald Heber');
INSERT INTO authors VALUES (3, 'John B.', 'Dykes', 'John B. Dykes');
INSERT INTO authors VALUES (4, 'Martín', 'Lutero', 'Martín Lutero');
INSERT INTO authors VALUES (5, 'Juan B.', 'Cabrera', 'Juan B. Cabrera');
INSERT INTO authors VALUES (6, 'Fanny J.', 'Crosby', 'Fanny J. Crosby');
INSERT INTO song_books VALUES (1, 'Himnario Bautista', NULL);
INSERT INTO topics VALUES (1, 'Grace');

-- Chords inline (OpenLP 2.4+), formatting tags, an optional split, a verse order.
INSERT INTO songs VALUES (1, 'Amazing Grace', 'New Britain',
'<?xml version=''1.0'' encoding=''UTF-8''?>
<song version="1.0"><lyrics><verse type="v" label="1"><![CDATA[[G]Amazing grace! How [C]sweet the [G]sound
That saved a wretch like me!
[---]
I once was lost, but now am {it}found{/it};
Was blind, but now I see.]]></verse><verse type="v" label="2"><![CDATA[''Twas grace that taught my heart to fear,
And grace my fears relieved;]]></verse><verse type="e" label="1"><![CDATA[{r}Praise God, praise God!{/r}]]></verse></lyrics></song>',
'v1 v2 v1 e1', 'Public domain', 'Sing slowly', '22025', 'Blue', 'amazing grace@new britain', 'amazing grace', '2024-01-07 10:00:00', '2024-01-07 10:00:00', 0, 0);
INSERT INTO authors_songs VALUES (1, 1, 'words');
INSERT INTO songs_topics VALUES (1, 1);

-- Two authors, one listed twice (words and music).
INSERT INTO songs VALUES (2, 'Holy, Holy, Holy', NULL,
'<?xml version=''1.0'' encoding=''UTF-8''?>
<song version="1.0"><lyrics><verse type="v" label="1"><![CDATA[Holy, holy, holy! Lord God Almighty!
Early in the morning our song shall rise to Thee;]]></verse><verse type="v" label="2"><![CDATA[Holy, holy, holy! All the saints adore Thee,
Casting down their golden crowns around the glassy sea;]]></verse></lyrics></song>',
'', 'Public domain', NULL, NULL, NULL, 'holy holy holy', 'holy holy holy', NULL, NULL, 0, 1);
INSERT INTO authors_songs VALUES (2, 2, 'words');
INSERT INTO authors_songs VALUES (3, 2, 'music');
INSERT INTO authors_songs VALUES (2, 2, 'words+music');

-- Spanish, with a song book entry, chorus and verse order in upper case.
INSERT INTO songs VALUES (3, 'Castillo Fuerte', NULL,
'<?xml version=''1.0'' encoding=''UTF-8''?>
<song version="1.0"><lyrics language="es"><verse type="v" label="1"><![CDATA[Castillo fuerte es nuestro Dios,
defensa y buen escudo.]]></verse><verse type="c" label="1"><![CDATA[{st}Su reino es,{/st} su reino es.]]></verse><verse type="v" label="2"><![CDATA[Nuestro valor es nada aquí,
con él todo es perdido;]]></verse></lyrics></song>',
'V1 C1 V2 C1', 'Dominio público', NULL, NULL, NULL, 'castillo fuerte', 'castillo fuerte', NULL, NULL, 0, 0);
INSERT INTO authors_songs VALUES (4, 3, '');
INSERT INTO authors_songs VALUES (5, 3, 'translation');
INSERT INTO songs_songbooks VALUES (1, 3, '25');

-- Already in the Asaf library (same title and lyrics): skipped.
INSERT INTO songs VALUES (4, 'Blessed Assurance', NULL,
'<?xml version=''1.0'' encoding=''UTF-8''?>
<song version="1.0"><lyrics><verse type="v" label="1"><![CDATA[Blessed assurance, Jesus is mine!
O what a foretaste of glory divine!]]></verse></lyrics></song>',
'', NULL, NULL, NULL, NULL, 'blessed assurance', 'blessed assurance', NULL, NULL, 0, 0);
INSERT INTO authors_songs VALUES (6, 4, 'words');

-- A temporary copy OpenLP made from an opened service file: not imported.
INSERT INTO songs VALUES (5, 'Amazing Grace', NULL,
'<?xml version=''1.0'' encoding=''UTF-8''?>
<song version="1.0"><lyrics><verse type="v" label="1"><![CDATA[Amazing grace! How sweet the sound]]></verse></lyrics></song>',
'', NULL, NULL, NULL, NULL, 'amazing grace', 'amazing grace', NULL, NULL, 1, 0);

-- Damaged lyrics: reported, the rest still imports.
INSERT INTO songs VALUES (6, 'Broken Song', NULL, '<song version="1.0"><lyrics><verse type="v"', '', NULL, NULL, NULL, NULL,
'broken song', 'broken song', NULL, NULL, 0, 0);
