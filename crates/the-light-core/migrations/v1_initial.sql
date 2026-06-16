-- Migração v1 — esquema do texto bíblico (read-only, embarcado).
-- Ver IMPLEMENTATION_PLAN.md §3. Dados do usuário NÃO ficam aqui (arquivos).

CREATE TABLE translations (
    id         TEXT PRIMARY KEY,   -- slug: 'kjv', 'alm1911'
    abbrev     TEXT NOT NULL,
    name       TEXT NOT NULL,
    language   TEXT NOT NULL,      -- 'en', 'pt'
    license    TEXT NOT NULL,      -- 'public-domain', 'cc-by', ...
    embeddable INTEGER NOT NULL    -- 1 = pode redistribuir
);

CREATE TABLE books (
    id             INTEGER PRIMARY KEY,
    translation_id TEXT NOT NULL REFERENCES translations(id) ON DELETE CASCADE,
    number         INTEGER NOT NULL,   -- ordem canônica 1..66
    name           TEXT NOT NULL,
    abbrev         TEXT NOT NULL,
    testament      TEXT NOT NULL,      -- 'OT' | 'NT'
    UNIQUE(translation_id, number)
);

CREATE TABLE verses (
    id             INTEGER PRIMARY KEY,
    translation_id TEXT NOT NULL REFERENCES translations(id) ON DELETE CASCADE,
    book_number    INTEGER NOT NULL,
    chapter        INTEGER NOT NULL,
    verse          INTEGER NOT NULL,
    text           TEXT NOT NULL,
    UNIQUE(translation_id, book_number, chapter, verse)
);

CREATE INDEX idx_verses_lookup
    ON verses(translation_id, book_number, chapter, verse);

CREATE INDEX idx_books_translation
    ON books(translation_id, number);

-- Índice full-text (FTS5) sobre o texto dos versículos.
-- 'remove_diacritics 2' permite busca em PT sem acento.
CREATE VIRTUAL TABLE verses_fts USING fts5(
    text,
    translation_id UNINDEXED,
    verse_id       UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Referências cruzadas (Treasury of Scripture Knowledge / OpenBible, fase 2).
CREATE TABLE cross_references (
    from_book      INTEGER NOT NULL,
    from_chapter   INTEGER NOT NULL,
    from_verse     INTEGER NOT NULL,
    to_book        INTEGER NOT NULL,
    to_chapter     INTEGER NOT NULL,
    to_verse_start INTEGER NOT NULL,
    to_verse_end   INTEGER NOT NULL,
    votes          INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_xref_from
    ON cross_references(from_book, from_chapter, from_verse);
