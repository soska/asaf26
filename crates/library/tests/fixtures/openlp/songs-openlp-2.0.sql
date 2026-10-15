-- An OpenLP 2.0 songs database (before songs schema 3): no `temporary`
-- column, authors_songs without author_type, the song book on the song.
CREATE TABLE authors (
    id INTEGER NOT NULL, first_name VARCHAR(128), last_name VARCHAR(128), display_name VARCHAR(255) NOT NULL,
    PRIMARY KEY (id)
);
CREATE TABLE song_books (id INTEGER NOT NULL, name VARCHAR(128) NOT NULL, publisher VARCHAR(128), PRIMARY KEY (id));
CREATE TABLE songs (
    id INTEGER NOT NULL,
    song_book_id INTEGER,
    title VARCHAR(255) NOT NULL,
    alternate_title VARCHAR(255),
    lyrics TEXT NOT NULL,
    verse_order VARCHAR(128),
    copyright VARCHAR(255),
    comments TEXT,
    ccli_number VARCHAR(64),
    song_number VARCHAR(64),
    theme_name VARCHAR(128),
    search_title VARCHAR(255) NOT NULL,
    search_lyrics TEXT NOT NULL,
    PRIMARY KEY (id),
    FOREIGN KEY(song_book_id) REFERENCES song_books (id)
);
CREATE TABLE authors_songs (
    author_id INTEGER NOT NULL, song_id INTEGER NOT NULL, PRIMARY KEY (author_id, song_id),
    FOREIGN KEY(author_id) REFERENCES authors (id), FOREIGN KEY(song_id) REFERENCES songs (id)
);
INSERT INTO authors VALUES (1, 'Isaac', 'Watts', 'Isaac Watts');
-- Spelled-out verse types, as very old versions wrote them.
INSERT INTO songs VALUES (1, NULL, 'Joy to the World', NULL,
'<?xml version=''1.0'' encoding=''UTF-8''?>
<song version="1.0"><lyrics language="en"><verse type="Verse" label="1"><![CDATA[Joy to the world, the Lord is come!
Let earth receive her King;]]></verse><verse type="Chorus" label="1"><![CDATA[And heaven and nature sing.]]></verse></lyrics></song>',
'v1 c1', 'Public domain', NULL, NULL, NULL, NULL, 'joy to the world', 'joy to the world');
INSERT INTO authors_songs VALUES (1, 1);
