-- Migração v2 — dados acadêmicos (línguas originais + léxico), read-only,
-- importados por `xtask import-scholarly` (NÃO embutidos no binário; vivem no
-- arquivo `biblia.sqlite`). Ver o plano de estudo acadêmico. Tudo é chaveado
-- pela tríade canônica (book_number, chapter, verse) — independente de tradução.

-- Procedência/licença de cada conjunto embarcado (anti-violação de licença).
-- O importador grava esta linha ANTES dos dados e recusa fontes não-embarcáveis.
CREATE TABLE scholarly_sources (
    id          TEXT PRIMARY KEY,   -- 'tahot','tagnt','tbesh','tbesg'
    name        TEXT NOT NULL,
    license     TEXT NOT NULL,      -- 'cc-by', 'public-domain', ...
    embeddable  INTEGER NOT NULL,   -- 1 = pode redistribuir (espelha is_embeddable)
    attribution TEXT NOT NULL,      -- string de atribuição exigida, verbatim
    url         TEXT NOT NULL,
    version     TEXT                -- tag/commit do release fixado (guarda de drift)
);

-- Tokens de língua original (uma linha por palavra). Independente de tradução:
-- chaveado pela referência canônica, então KJV/ALM1911/etc. unem-se aqui por ref.
CREATE TABLE original_tokens (
    id          INTEGER PRIMARY KEY,
    testament   TEXT NOT NULL,      -- 'OT' (hebraico) | 'NT' (grego)
    book_number INTEGER NOT NULL,   -- canônico 1..66, casa com verses.book_number
    chapter     INTEGER NOT NULL,
    verse       INTEGER NOT NULL,
    word_index  INTEGER NOT NULL,   -- posição 0-based da palavra no versículo
    surface     TEXT NOT NULL,      -- palavra hebraica/grega como impressa
    translit    TEXT,
    lemma       TEXT,
    strongs     TEXT,               -- 'H7225','G3056' (desambiguado, p/ unir ao léxico)
    strongs_raw TEXT,               -- dStrong completo como veio (procedência exata)
    morph_code  TEXT,               -- ex.: 'HNcfsa' / 'V-PAI-3S'
    gloss       TEXT,               -- glosa curta (idioma da fonte, normalmente EN)
    source_id   TEXT NOT NULL REFERENCES scholarly_sources(id),
    UNIQUE(book_number, chapter, verse, word_index, source_id)
);

CREATE INDEX idx_tokens_ref     ON original_tokens(book_number, chapter, verse);
CREATE INDEX idx_tokens_strongs ON original_tokens(strongs);

-- Léxico: número de Strong -> definição. Uma linha por (strongs, fonte). A chave
-- guarda o Strong **desambiguado** quando a fonte o fornece (H7225a != H7225b),
-- para nunca fundir lemas distintos numa só glosa.
CREATE TABLE lexicon (
    strongs     TEXT NOT NULL,      -- 'H1','G3056' (desambiguado quando há)
    lemma       TEXT,
    translit    TEXT,
    pron        TEXT,
    gloss       TEXT,               -- glosa breve (idioma da fonte)
    gloss_pt    TEXT,               -- glosa em português (quando disponível)
    definition  TEXT,               -- definição mais longa (TBESH/TBESG)
    derivation  TEXT,
    source_id   TEXT NOT NULL REFERENCES scholarly_sources(id),
    PRIMARY KEY (strongs, source_id)
);

CREATE INDEX idx_lexicon_strongs ON lexicon(strongs);

-- Legenda dos códigos de morfologia (expansão legível de cada código).
-- Populada opcionalmente; a glosa/lema já bastam ao prompt mesmo sem ela.
CREATE TABLE morph_legend (
    code       TEXT NOT NULL,       -- 'HNcfsa', 'V-PAI-3S'
    language   TEXT NOT NULL,       -- 'hbo' | 'grc'
    expansion  TEXT NOT NULL,       -- 'Substantivo, comum, feminino, singular, absoluto'
    source_id  TEXT NOT NULL REFERENCES scholarly_sources(id),
    PRIMARY KEY (code, language, source_id)
);

-- Mapa de versificação: numeração da fonte (massorética/crítica) -> canônica
-- (a das traduções embarcadas, p.ex. KJV/ALM1911). Resolve o deslocamento dos
-- títulos dos Salmos e ~30 passagens conhecidas, para o léxico não falhar em
-- silêncio. `scheme` identifica a convenção de origem (ex.: 'mt' massorético).
CREATE TABLE versification_map (
    scheme       TEXT NOT NULL,     -- 'mt' (massorético) | 'na' (Nestle-Aland) | ...
    book_number  INTEGER NOT NULL,  -- canônico 1..66
    src_chapter  INTEGER NOT NULL,
    src_verse    INTEGER NOT NULL,
    canon_chapter INTEGER NOT NULL,
    canon_verse   INTEGER NOT NULL,
    PRIMARY KEY (scheme, book_number, src_chapter, src_verse)
);

CREATE INDEX idx_versif_canon
    ON versification_map(scheme, book_number, canon_chapter, canon_verse);
