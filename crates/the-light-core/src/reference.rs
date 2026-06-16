//! Parser de referências bíblicas PT/EN e tabela canônica dos 66 livros.
//!
//! Aceita formas como `Jo 3.16`, `John 3:16`, `Gn 1.1-3`, `Sl 23`,
//! `1Co 13.4-7`, `II Coríntios 5:17` e listas separadas por `;`.
//!
//! Ambiguidades reais do português são tratadas preservando acentos na busca
//! primária e só caindo para a forma sem acento quando não há colisão:
//! `Jó`→Jó (Job) e `Jo`→João (John); `Jn`→Jonas, enquanto João usa `Jo`/`Jhn`.

use crate::model::{Reference, Testament, VerseRange};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Metadados canônicos de um livro da Bíblia (cânon protestante, 66 livros).
#[derive(Debug, Clone, Copy)]
pub struct BookInfo {
    /// Número canônico, `1..=66`.
    pub number: u8,
    /// Nome em inglês.
    pub name_en: &'static str,
    /// Nome em português.
    pub name_pt: &'static str,
    /// Abreviação canônica em inglês.
    pub abbrev_en: &'static str,
    /// Abreviação canônica em português (padrão Almeida).
    pub abbrev_pt: &'static str,
    /// Testamento.
    pub testament: Testament,
    /// Aliases extras (PT+EN), grafias livres adicionais para o parser.
    pub aliases: &'static [&'static str],
}

/// Tabela canônica dos 66 livros, em ordem.
pub const BOOKS: [BookInfo; 66] = [
    BookInfo {
        number: 1,
        name_en: "Genesis",
        name_pt: "Gênesis",
        abbrev_en: "Gen",
        abbrev_pt: "Gn",
        testament: Testament::Old,
        aliases: &["ge", "gen", "genesis"],
    },
    BookInfo {
        number: 2,
        name_en: "Exodus",
        name_pt: "Êxodo",
        abbrev_en: "Exod",
        abbrev_pt: "Êx",
        testament: Testament::Old,
        aliases: &["ex", "exo", "exod", "exodo"],
    },
    BookInfo {
        number: 3,
        name_en: "Leviticus",
        name_pt: "Levítico",
        abbrev_en: "Lev",
        abbrev_pt: "Lv",
        testament: Testament::Old,
        aliases: &["levitico", "leviticus"],
    },
    BookInfo {
        number: 4,
        name_en: "Numbers",
        name_pt: "Números",
        abbrev_en: "Num",
        abbrev_pt: "Nm",
        testament: Testament::Old,
        aliases: &["nu", "num", "numeros", "numbers"],
    },
    BookInfo {
        number: 5,
        name_en: "Deuteronomy",
        name_pt: "Deuteronômio",
        abbrev_en: "Deut",
        abbrev_pt: "Dt",
        testament: Testament::Old,
        aliases: &["de", "deut", "deuteronomio", "deuteronomy"],
    },
    BookInfo {
        number: 6,
        name_en: "Joshua",
        name_pt: "Josué",
        abbrev_en: "Josh",
        abbrev_pt: "Js",
        testament: Testament::Old,
        aliases: &["jos", "josh", "josue", "joshua"],
    },
    BookInfo {
        number: 7,
        name_en: "Judges",
        name_pt: "Juízes",
        abbrev_en: "Judg",
        abbrev_pt: "Jz",
        testament: Testament::Old,
        aliases: &["judg", "juizes", "judges"],
    },
    BookInfo {
        number: 8,
        name_en: "Ruth",
        name_pt: "Rute",
        abbrev_en: "Ruth",
        abbrev_pt: "Rt",
        testament: Testament::Old,
        aliases: &["ru", "rute", "ruth"],
    },
    BookInfo {
        number: 9,
        name_en: "1 Samuel",
        name_pt: "1 Samuel",
        abbrev_en: "1Sam",
        abbrev_pt: "1Sm",
        testament: Testament::Old,
        aliases: &["1sa", "1sam", "1samuel"],
    },
    BookInfo {
        number: 10,
        name_en: "2 Samuel",
        name_pt: "2 Samuel",
        abbrev_en: "2Sam",
        abbrev_pt: "2Sm",
        testament: Testament::Old,
        aliases: &["2sa", "2sam", "2samuel"],
    },
    BookInfo {
        number: 11,
        name_en: "1 Kings",
        name_pt: "1 Reis",
        abbrev_en: "1Kgs",
        abbrev_pt: "1Rs",
        testament: Testament::Old,
        aliases: &["1ki", "1kgs", "1kings", "1reis"],
    },
    BookInfo {
        number: 12,
        name_en: "2 Kings",
        name_pt: "2 Reis",
        abbrev_en: "2Kgs",
        abbrev_pt: "2Rs",
        testament: Testament::Old,
        aliases: &["2ki", "2kgs", "2kings", "2reis"],
    },
    BookInfo {
        number: 13,
        name_en: "1 Chronicles",
        name_pt: "1 Crônicas",
        abbrev_en: "1Chr",
        abbrev_pt: "1Cr",
        testament: Testament::Old,
        aliases: &["1ch", "1chr", "1chronicles", "1cronicas"],
    },
    BookInfo {
        number: 14,
        name_en: "2 Chronicles",
        name_pt: "2 Crônicas",
        abbrev_en: "2Chr",
        abbrev_pt: "2Cr",
        testament: Testament::Old,
        aliases: &["2ch", "2chr", "2chronicles", "2cronicas"],
    },
    BookInfo {
        number: 15,
        name_en: "Ezra",
        name_pt: "Esdras",
        abbrev_en: "Ezra",
        abbrev_pt: "Ed",
        testament: Testament::Old,
        aliases: &["esd", "esdras", "ezra"],
    },
    BookInfo {
        number: 16,
        name_en: "Nehemiah",
        name_pt: "Neemias",
        abbrev_en: "Neh",
        abbrev_pt: "Ne",
        testament: Testament::Old,
        aliases: &["neh", "neemias", "nehemiah"],
    },
    BookInfo {
        number: 17,
        name_en: "Esther",
        name_pt: "Ester",
        abbrev_en: "Esth",
        abbrev_pt: "Et",
        testament: Testament::Old,
        aliases: &["est", "esth", "ester", "esther"],
    },
    BookInfo {
        number: 18,
        name_en: "Job",
        name_pt: "Jó",
        abbrev_en: "Job",
        abbrev_pt: "Jó",
        testament: Testament::Old,
        aliases: &["job"],
    },
    BookInfo {
        number: 19,
        name_en: "Psalms",
        name_pt: "Salmos",
        abbrev_en: "Ps",
        abbrev_pt: "Sl",
        testament: Testament::Old,
        aliases: &["psa", "psalm", "psalms", "salmo", "salmos"],
    },
    BookInfo {
        number: 20,
        name_en: "Proverbs",
        name_pt: "Provérbios",
        abbrev_en: "Prov",
        abbrev_pt: "Pv",
        testament: Testament::Old,
        aliases: &["pr", "prov", "proverbios", "proverbs"],
    },
    BookInfo {
        number: 21,
        name_en: "Ecclesiastes",
        name_pt: "Eclesiastes",
        abbrev_en: "Eccl",
        abbrev_pt: "Ec",
        testament: Testament::Old,
        aliases: &["ecc", "eccl", "eclesiastes", "ecclesiastes", "qo", "coelet"],
    },
    BookInfo {
        number: 22,
        name_en: "Song of Solomon",
        name_pt: "Cânticos",
        abbrev_en: "Song",
        abbrev_pt: "Ct",
        testament: Testament::Old,
        aliases: &[
            "song",
            "songofsongs",
            "songofsolomon",
            "canticles",
            "cantico",
            "canticos",
            "cantares",
            "canticodoscanticos",
        ],
    },
    BookInfo {
        number: 23,
        name_en: "Isaiah",
        name_pt: "Isaías",
        abbrev_en: "Isa",
        abbrev_pt: "Is",
        testament: Testament::Old,
        aliases: &["isa", "isaias", "isaiah"],
    },
    BookInfo {
        number: 24,
        name_en: "Jeremiah",
        name_pt: "Jeremias",
        abbrev_en: "Jer",
        abbrev_pt: "Jr",
        testament: Testament::Old,
        aliases: &["jer", "jeremias", "jeremiah"],
    },
    BookInfo {
        number: 25,
        name_en: "Lamentations",
        name_pt: "Lamentações",
        abbrev_en: "Lam",
        abbrev_pt: "Lm",
        testament: Testament::Old,
        aliases: &["lam", "lamentacoes", "lamentations"],
    },
    BookInfo {
        number: 26,
        name_en: "Ezekiel",
        name_pt: "Ezequiel",
        abbrev_en: "Ezek",
        abbrev_pt: "Ez",
        testament: Testament::Old,
        aliases: &["eze", "ezek", "ezequiel", "ezekiel"],
    },
    BookInfo {
        number: 27,
        name_en: "Daniel",
        name_pt: "Daniel",
        abbrev_en: "Dan",
        abbrev_pt: "Dn",
        testament: Testament::Old,
        aliases: &["dan", "daniel"],
    },
    BookInfo {
        number: 28,
        name_en: "Hosea",
        name_pt: "Oseias",
        abbrev_en: "Hos",
        abbrev_pt: "Os",
        testament: Testament::Old,
        aliases: &["hos", "oseias", "hosea"],
    },
    BookInfo {
        number: 29,
        name_en: "Joel",
        name_pt: "Joel",
        abbrev_en: "Joel",
        abbrev_pt: "Jl",
        testament: Testament::Old,
        aliases: &["joe", "joel"],
    },
    BookInfo {
        number: 30,
        name_en: "Amos",
        name_pt: "Amós",
        abbrev_en: "Amos",
        abbrev_pt: "Am",
        testament: Testament::Old,
        aliases: &["amo", "amos"],
    },
    BookInfo {
        number: 31,
        name_en: "Obadiah",
        name_pt: "Obadias",
        abbrev_en: "Obad",
        abbrev_pt: "Ob",
        testament: Testament::Old,
        aliases: &["oba", "obad", "obadias", "obadiah"],
    },
    BookInfo {
        number: 32,
        name_en: "Jonah",
        name_pt: "Jonas",
        abbrev_en: "Jonah",
        abbrev_pt: "Jn",
        testament: Testament::Old,
        aliases: &["jon", "jonas", "jonah"],
    },
    BookInfo {
        number: 33,
        name_en: "Micah",
        name_pt: "Miqueias",
        abbrev_en: "Mic",
        abbrev_pt: "Mq",
        testament: Testament::Old,
        aliases: &["mic", "miqueias", "micah"],
    },
    BookInfo {
        number: 34,
        name_en: "Nahum",
        name_pt: "Naum",
        abbrev_en: "Nah",
        abbrev_pt: "Na",
        testament: Testament::Old,
        aliases: &["nah", "naum", "nahum"],
    },
    BookInfo {
        number: 35,
        name_en: "Habakkuk",
        name_pt: "Habacuque",
        abbrev_en: "Hab",
        abbrev_pt: "Hc",
        testament: Testament::Old,
        aliases: &["hab", "habacuque", "habakkuk"],
    },
    BookInfo {
        number: 36,
        name_en: "Zephaniah",
        name_pt: "Sofonias",
        abbrev_en: "Zeph",
        abbrev_pt: "Sf",
        testament: Testament::Old,
        aliases: &["zep", "zeph", "sofonias", "zephaniah"],
    },
    BookInfo {
        number: 37,
        name_en: "Haggai",
        name_pt: "Ageu",
        abbrev_en: "Hag",
        abbrev_pt: "Ag",
        testament: Testament::Old,
        aliases: &["hag", "ageu", "haggai"],
    },
    BookInfo {
        number: 38,
        name_en: "Zechariah",
        name_pt: "Zacarias",
        abbrev_en: "Zech",
        abbrev_pt: "Zc",
        testament: Testament::Old,
        aliases: &["zec", "zech", "zacarias", "zechariah"],
    },
    BookInfo {
        number: 39,
        name_en: "Malachi",
        name_pt: "Malaquias",
        abbrev_en: "Mal",
        abbrev_pt: "Ml",
        testament: Testament::Old,
        aliases: &["mal", "malaquias", "malachi"],
    },
    BookInfo {
        number: 40,
        name_en: "Matthew",
        name_pt: "Mateus",
        abbrev_en: "Matt",
        abbrev_pt: "Mt",
        testament: Testament::New,
        aliases: &["mat", "matt", "mateus", "matthew"],
    },
    BookInfo {
        number: 41,
        name_en: "Mark",
        name_pt: "Marcos",
        abbrev_en: "Mark",
        abbrev_pt: "Mc",
        testament: Testament::New,
        aliases: &["mar", "mark", "marcos", "mr"],
    },
    BookInfo {
        number: 42,
        name_en: "Luke",
        name_pt: "Lucas",
        abbrev_en: "Luke",
        abbrev_pt: "Lc",
        testament: Testament::New,
        aliases: &["luk", "luke", "lucas"],
    },
    BookInfo {
        number: 43,
        name_en: "John",
        name_pt: "João",
        abbrev_en: "John",
        abbrev_pt: "Jo",
        testament: Testament::New,
        aliases: &["joh", "jhn", "john", "joao"],
    },
    BookInfo {
        number: 44,
        name_en: "Acts",
        name_pt: "Atos",
        abbrev_en: "Acts",
        abbrev_pt: "At",
        testament: Testament::New,
        aliases: &["act", "acts", "atos"],
    },
    BookInfo {
        number: 45,
        name_en: "Romans",
        name_pt: "Romanos",
        abbrev_en: "Rom",
        abbrev_pt: "Rm",
        testament: Testament::New,
        aliases: &["ro", "rom", "romanos", "romans"],
    },
    BookInfo {
        number: 46,
        name_en: "1 Corinthians",
        name_pt: "1 Coríntios",
        abbrev_en: "1Cor",
        abbrev_pt: "1Co",
        testament: Testament::New,
        aliases: &["1co", "1cor", "1corinthians", "1corintios"],
    },
    BookInfo {
        number: 47,
        name_en: "2 Corinthians",
        name_pt: "2 Coríntios",
        abbrev_en: "2Cor",
        abbrev_pt: "2Co",
        testament: Testament::New,
        aliases: &["2co", "2cor", "2corinthians", "2corintios"],
    },
    BookInfo {
        number: 48,
        name_en: "Galatians",
        name_pt: "Gálatas",
        abbrev_en: "Gal",
        abbrev_pt: "Gl",
        testament: Testament::New,
        aliases: &["gal", "galatas", "galatians"],
    },
    BookInfo {
        number: 49,
        name_en: "Ephesians",
        name_pt: "Efésios",
        abbrev_en: "Eph",
        abbrev_pt: "Ef",
        testament: Testament::New,
        aliases: &["eph", "efesios", "ephesians"],
    },
    BookInfo {
        number: 50,
        name_en: "Philippians",
        name_pt: "Filipenses",
        abbrev_en: "Phil",
        abbrev_pt: "Fp",
        testament: Testament::New,
        aliases: &["php", "phil", "fil", "flp", "filipenses", "philippians"],
    },
    BookInfo {
        number: 51,
        name_en: "Colossians",
        name_pt: "Colossenses",
        abbrev_en: "Col",
        abbrev_pt: "Cl",
        testament: Testament::New,
        aliases: &["col", "colossenses", "colossians"],
    },
    BookInfo {
        number: 52,
        name_en: "1 Thessalonians",
        name_pt: "1 Tessalonicenses",
        abbrev_en: "1Thess",
        abbrev_pt: "1Ts",
        testament: Testament::New,
        aliases: &["1th", "1thess", "1thessalonians", "1tessalonicenses"],
    },
    BookInfo {
        number: 53,
        name_en: "2 Thessalonians",
        name_pt: "2 Tessalonicenses",
        abbrev_en: "2Thess",
        abbrev_pt: "2Ts",
        testament: Testament::New,
        aliases: &["2th", "2thess", "2thessalonians", "2tessalonicenses"],
    },
    BookInfo {
        number: 54,
        name_en: "1 Timothy",
        name_pt: "1 Timóteo",
        abbrev_en: "1Tim",
        abbrev_pt: "1Tm",
        testament: Testament::New,
        aliases: &["1ti", "1tim", "1timothy", "1timoteo"],
    },
    BookInfo {
        number: 55,
        name_en: "2 Timothy",
        name_pt: "2 Timóteo",
        abbrev_en: "2Tim",
        abbrev_pt: "2Tm",
        testament: Testament::New,
        aliases: &["2ti", "2tim", "2timothy", "2timoteo"],
    },
    BookInfo {
        number: 56,
        name_en: "Titus",
        name_pt: "Tito",
        abbrev_en: "Titus",
        abbrev_pt: "Tt",
        testament: Testament::New,
        aliases: &["tit", "tito", "titus"],
    },
    BookInfo {
        number: 57,
        name_en: "Philemon",
        name_pt: "Filemom",
        abbrev_en: "Phlm",
        abbrev_pt: "Fm",
        testament: Testament::New,
        aliases: &["phm", "phlm", "philem", "filemom", "filemon"],
    },
    BookInfo {
        number: 58,
        name_en: "Hebrews",
        name_pt: "Hebreus",
        abbrev_en: "Heb",
        abbrev_pt: "Hb",
        testament: Testament::New,
        aliases: &["heb", "hebreus", "hebrews"],
    },
    BookInfo {
        number: 59,
        name_en: "James",
        name_pt: "Tiago",
        abbrev_en: "Jas",
        abbrev_pt: "Tg",
        testament: Testament::New,
        aliases: &["jas", "jm", "tiago", "james"],
    },
    BookInfo {
        number: 60,
        name_en: "1 Peter",
        name_pt: "1 Pedro",
        abbrev_en: "1Pet",
        abbrev_pt: "1Pe",
        testament: Testament::New,
        aliases: &["1pe", "1pet", "1peter", "1pedro", "1pd"],
    },
    BookInfo {
        number: 61,
        name_en: "2 Peter",
        name_pt: "2 Pedro",
        abbrev_en: "2Pet",
        abbrev_pt: "2Pe",
        testament: Testament::New,
        aliases: &["2pe", "2pet", "2peter", "2pedro", "2pd"],
    },
    BookInfo {
        number: 62,
        name_en: "1 John",
        name_pt: "1 João",
        abbrev_en: "1John",
        abbrev_pt: "1Jo",
        testament: Testament::New,
        aliases: &["1jn", "1joh", "1john", "1joao"],
    },
    BookInfo {
        number: 63,
        name_en: "2 John",
        name_pt: "2 João",
        abbrev_en: "2John",
        abbrev_pt: "2Jo",
        testament: Testament::New,
        aliases: &["2jn", "2joh", "2john", "2joao"],
    },
    BookInfo {
        number: 64,
        name_en: "3 John",
        name_pt: "3 João",
        abbrev_en: "3John",
        abbrev_pt: "3Jo",
        testament: Testament::New,
        aliases: &["3jn", "3joh", "3john", "3joao"],
    },
    BookInfo {
        number: 65,
        name_en: "Jude",
        name_pt: "Judas",
        abbrev_en: "Jude",
        abbrev_pt: "Jd",
        testament: Testament::New,
        aliases: &["jude", "judas"],
    },
    BookInfo {
        number: 66,
        name_en: "Revelation",
        name_pt: "Apocalipse",
        abbrev_en: "Rev",
        abbrev_pt: "Ap",
        testament: Testament::New,
        aliases: &[
            "re",
            "rev",
            "apc",
            "apoc",
            "apocalipse",
            "revelation",
            "revelations",
        ],
    },
];

/// Número de capítulos de cada livro, em ordem canônica (1..=66).
/// Total = 1189 capítulos. Usado pelos planos de leitura (independe da versão).
pub const CHAPTERS: [u16; 66] = [
    50, 40, 27, 36, 34, 24, 21, 4, 31, 24, 22, 25, 29, 36, 10, 13, 10, 42, 150, 31, 12, 8, 66, 52,
    5, 48, 12, 14, 3, 9, 1, 4, 7, 3, 3, 3, 2, 14, 4, 28, 16, 24, 21, 28, 16, 16, 13, 6, 6, 4, 4, 5,
    3, 6, 4, 3, 1, 13, 5, 5, 3, 5, 1, 1, 1, 22,
];

/// Retorna os metadados de um livro pelo número canônico (`1..=66`).
pub fn book_info(number: u8) -> Option<&'static BookInfo> {
    BOOKS.get(number.checked_sub(1)? as usize)
}

/// Número canônico de capítulos de um livro (`0` se fora de `1..=66`).
pub fn chapters_in_book(number: u8) -> u16 {
    number
        .checked_sub(1)
        .and_then(|i| CHAPTERS.get(i as usize).copied())
        .unwrap_or(0)
}

/// Resolve um nome/abreviação de livro (PT ou EN, com ou sem acento) para o
/// número canônico. Retorna `None` se desconhecido.
pub fn book_number(token: &str) -> Option<u8> {
    let key = normalize(token);
    if key.is_empty() {
        return None;
    }
    alias_map().get(&key).copied()
}

/// Normaliza um token: minúsculas, sem espaços/pontuação, mantendo acentos.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
        }
    }
    out
}

/// Remove diacríticos de uma string já normalizada (minúscula, sem espaços).
fn fold_ascii(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// Mapa (alias normalizado → número do livro), construído uma única vez.
fn alias_map() -> &'static HashMap<String, u8> {
    static MAP: OnceLock<HashMap<String, u8>> = OnceLock::new();
    MAP.get_or_init(build_alias_map)
}

fn build_alias_map() -> HashMap<String, u8> {
    // Coleta todas as grafias (com acento) por livro.
    let mut raw: Vec<(String, u8)> = Vec::new();
    let push = |s: &str, n: u8, raw: &mut Vec<(String, u8)>| {
        let k = normalize(s);
        if !k.is_empty() {
            raw.push((k, n));
        }
    };
    for b in &BOOKS {
        push(b.name_en, b.number, &mut raw);
        push(b.name_pt, b.number, &mut raw);
        push(b.abbrev_en, b.number, &mut raw);
        push(b.abbrev_pt, b.number, &mut raw);
        for a in b.aliases {
            push(a, b.number, &mut raw);
        }
    }

    // Variantes romanas (1→I, 2→II, 3→III), camada *best-effort* — não podem
    // sobrescrever um alias primário. Usa `chars()` (não índice de byte) porque
    // chaves podem começar com caractere multibyte como `ê` (ex.: "êx").
    let mut roman: Vec<(String, u8)> = Vec::new();
    for (k, n) in &raw {
        let mut chars = k.chars();
        let first = chars.next();
        let rest = chars.as_str();
        match first {
            Some('1') => roman.push((format!("i{rest}"), *n)),
            Some('2') => roman.push((format!("ii{rest}"), *n)),
            Some('3') => roman.push((format!("iii{rest}"), *n)),
            _ => {}
        }
    }

    // Passo 1: aliases primários (com acento). Colisão entre dois primários de
    // livros diferentes é um erro de dados → `debug_assert`.
    let mut map: HashMap<String, u8> = HashMap::new();
    for (k, n) in &raw {
        match map.get(k) {
            Some(&existing) if existing != *n => {
                debug_assert!(
                    false,
                    "colisão de alias primário `{k}`: livro {existing} vs {n}"
                );
            }
            _ => {
                map.insert(k.clone(), *n);
            }
        }
    }

    // Passo 2: variantes romanas — só preenchem chaves livres (ex.: "isa" da
    // expansão de "1sa" cede para Isaías, que já a ocupa).
    for (k, n) in &roman {
        map.entry(k.clone()).or_insert(*n);
    }

    // Passo 3: dobra sem acento de todas as grafias — só preenche chaves livres,
    // preservando desambiguações (`jo`→João já ocupa a dobra de `jó`→Jó).
    for (k, n) in raw.iter().chain(roman.iter()) {
        let folded = fold_ascii(k);
        if folded != *k {
            map.entry(folded).or_insert(*n);
        }
    }

    map
}

/// Regex que decompõe uma referência única em livro/capítulo/versículos.
fn reference_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // `\p{M}` (marcas combinantes) no corpo do nome permite entrada UTF-8
        // decomposta (NFD); `normalize()` descarta essas marcas depois.
        Regex::new(
            r"^\s*(?P<book>[123]?\s*\p{L}[\p{L}\p{M}.\s]*?)\s*(?P<chap>\d+)(?:\s*[:.]\s*(?P<v1>\d+)(?:\s*-\s*(?P<v2>\d+))?)?\s*$",
        )
        .expect("regex de referência válida")
    })
}

/// Erro ao analisar uma referência bíblica.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReferenceError {
    /// Entrada vazia ou só espaços.
    #[error("referência vazia")]
    Empty,
    /// Não casa com a gramática de referências.
    #[error("referência malformada: {0:?}")]
    Malformed(String),
    /// Livro não reconhecido.
    #[error("livro desconhecido: {0:?}")]
    UnknownBook(String),
    /// Número fora do intervalo válido (`u16`) ou capítulo/versículo zero.
    #[error("número inválido em: {0:?}")]
    InvalidNumber(String),
    /// Intervalo com fim antes do início.
    #[error("intervalo invertido: {start}-{end}")]
    RangeOrder {
        /// Início informado.
        start: u16,
        /// Fim informado.
        end: u16,
    },
}

/// Analisa uma única referência bíblica em PT ou EN.
///
/// # Exemplos
/// ```
/// use the_light_core::reference::parse_reference;
/// use the_light_core::model::VerseRange;
/// let r = parse_reference("Jo 3.16").unwrap();
/// assert_eq!(r.book, 43);
/// assert_eq!(r.chapter, 3);
/// assert_eq!(r.verses, VerseRange::Single(16));
/// ```
pub fn parse_reference(input: &str) -> Result<Reference, ReferenceError> {
    if input.trim().is_empty() {
        return Err(ReferenceError::Empty);
    }
    let caps = reference_regex()
        .captures(input)
        .ok_or_else(|| ReferenceError::Malformed(input.to_string()))?;

    let book_token = &caps["book"];
    let book = book_number(book_token)
        .ok_or_else(|| ReferenceError::UnknownBook(book_token.trim().to_string()))?;

    let chapter: u16 = caps["chap"]
        .parse()
        .map_err(|_| ReferenceError::InvalidNumber(input.to_string()))?;
    if chapter == 0 {
        return Err(ReferenceError::InvalidNumber(input.to_string()));
    }

    let v1 = caps
        .name("v1")
        .map(|m| m.as_str().parse::<u16>())
        .transpose()
        .map_err(|_| ReferenceError::InvalidNumber(input.to_string()))?;
    let v2 = caps
        .name("v2")
        .map(|m| m.as_str().parse::<u16>())
        .transpose()
        .map_err(|_| ReferenceError::InvalidNumber(input.to_string()))?;

    let verses = match (v1, v2) {
        (None, _) => VerseRange::WholeChapter,
        (Some(a), None) => {
            if a == 0 {
                return Err(ReferenceError::InvalidNumber(input.to_string()));
            }
            VerseRange::Single(a)
        }
        (Some(a), Some(b)) => {
            if a == 0 || b == 0 {
                return Err(ReferenceError::InvalidNumber(input.to_string()));
            }
            if a > b {
                return Err(ReferenceError::RangeOrder { start: a, end: b });
            }
            if a == b {
                VerseRange::Single(a)
            } else {
                VerseRange::Range { start: a, end: b }
            }
        }
    };

    Ok(Reference {
        book,
        chapter,
        verses,
    })
}

/// Analisa uma lista de referências separadas por `;` ou nova linha.
///
/// Falha no primeiro item inválido.
pub fn parse_references(input: &str) -> Result<Vec<Reference>, ReferenceError> {
    let mut out = Vec::new();
    for piece in input.split([';', '\n']) {
        if piece.trim().is_empty() {
            continue;
        }
        out.push(parse_reference(piece)?);
    }
    if out.is_empty() {
        return Err(ReferenceError::Empty);
    }
    Ok(out)
}

/// Uma referência detectada dentro de um texto livre (prosa), com o intervalo
/// de bytes que ela ocupa no texto original (para realce inline).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedRef {
    /// Intervalo de bytes da referência no texto de origem.
    pub range: std::ops::Range<usize>,
    /// Referência reconhecida.
    pub reference: Reference,
}

/// Regex de varredura: nome de livro (qualquer grafia conhecida) seguido de
/// `cap[:.]ver[-ver]?`. Construída uma única vez, com as grafias ordenadas da
/// mais longa para a mais curta (o motor é *leftmost-first*: assim "1 Coríntios"
/// vence "Coríntios" e o numeral inicial não vira um capítulo solto).
fn scan_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let mut spellings: Vec<&'static str> = Vec::new();
        for b in &BOOKS {
            spellings.push(b.name_en);
            spellings.push(b.name_pt);
            spellings.push(b.abbrev_en);
            spellings.push(b.abbrev_pt);
            for a in b.aliases {
                spellings.push(a);
            }
        }
        spellings.sort_by(|a, b| {
            b.chars()
                .count()
                .cmp(&a.chars().count())
                .then_with(|| a.cmp(b))
        });
        spellings.dedup();
        let alt = spellings
            .iter()
            .map(|s| regex::escape(s))
            .collect::<Vec<_>>()
            .join("|");
        let pat = format!(
            r"(?i)\b(?P<book>{alt})\.?\s*(?P<chap>\d{{1,3}})(?:\s*[:.]\s*(?P<v1>\d{{1,3}})(?:\s*[-–]\s*(?P<v2>\d{{1,3}}))?)?"
        );
        Regex::new(&pat).expect("regex de scan de referências válida")
    })
}

/// Varre um texto livre e devolve todas as referências bíblicas citadas, na
/// ordem em que aparecem, com seus intervalos de bytes.
///
/// Reduz falsos positivos: abreviações curtas (≤3 letras) só contam quando há
/// um versículo explícito (ex.: `Gn 1.1` conta, `am 7` não); capítulos fora do
/// intervalo do livro são descartados.
///
/// # Exemplos
/// ```
/// use the_light_core::reference::scan_references;
/// let refs = scan_references("Como diz João 3:16 e Romanos 5.8.");
/// assert_eq!(refs.len(), 2);
/// assert_eq!(refs[0].reference.book, 43);
/// assert_eq!(refs[1].reference.chapter, 5);
/// ```
pub fn scan_references(text: &str) -> Vec<ScannedRef> {
    let mut out = Vec::new();
    for caps in scan_regex().captures_iter(text) {
        let whole = caps.get(0).unwrap();
        let book_tok = &caps["book"];
        let Some(book) = book_number(book_tok) else {
            continue;
        };
        let Ok(chapter) = caps["chap"].parse::<u16>() else {
            continue;
        };
        if chapter == 0 || chapter > chapters_in_book(book) {
            continue;
        }
        let v1 = caps.name("v1").and_then(|m| m.as_str().parse::<u16>().ok());
        let v2 = caps.name("v2").and_then(|m| m.as_str().parse::<u16>().ok());

        // Abreviação curta sem versículo é ruído em prosa: exige separador.
        let book_len = book_tok.chars().filter(|c| c.is_alphanumeric()).count();
        if book_len <= 3 && v1.is_none() {
            continue;
        }

        let verses = match (v1, v2) {
            (None, _) => VerseRange::WholeChapter,
            (Some(a), None) if a > 0 => VerseRange::Single(a),
            (Some(a), Some(b)) if a > 0 && b > 0 && a <= b => {
                if a == b {
                    VerseRange::Single(a)
                } else {
                    VerseRange::Range { start: a, end: b }
                }
            }
            _ => continue,
        };

        out.push(ScannedRef {
            range: whole.range(),
            reference: Reference {
                book,
                chapter,
                verses,
            },
        });
    }
    out
}

/// Idioma de exibição para formatar uma referência.
pub use crate::model::Lang;

/// Formata uma referência de forma legível no idioma dado.
///
/// PT usa `.` entre capítulo e versículo; EN usa `:`.
pub fn format_reference(r: &Reference, lang: Lang) -> String {
    let info = book_info(r.book);
    let name = match (info, lang) {
        (Some(b), Lang::Pt) => b.name_pt,
        (Some(b), Lang::En) => b.name_en,
        (None, _) => "?",
    };
    let sep = match lang {
        Lang::Pt => '.',
        Lang::En => ':',
    };
    match r.verses {
        VerseRange::WholeChapter => format!("{name} {}", r.chapter),
        VerseRange::Single(v) => format!("{name} {}{sep}{v}", r.chapter),
        VerseRange::Range { start, end } => {
            format!("{name} {}{sep}{start}-{end}", r.chapter)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VerseRange;

    fn r(book: u8, chapter: u16, verses: VerseRange) -> Reference {
        Reference {
            book,
            chapter,
            verses,
        }
    }

    #[test]
    fn alias_map_has_no_conflicts_and_covers_66_books() {
        // Construir o mapa em modo debug dispara `debug_assert` em colisão real.
        let map = alias_map();
        // Cada um dos 66 livros deve ser alcançável pelo nome em inglês.
        for b in &BOOKS {
            assert_eq!(book_number(b.name_en), Some(b.number), "en {}", b.name_en);
            assert_eq!(book_number(b.name_pt), Some(b.number), "pt {}", b.name_pt);
            assert_eq!(
                book_number(b.abbrev_en),
                Some(b.number),
                "abbr_en {}",
                b.abbrev_en
            );
            assert_eq!(
                book_number(b.abbrev_pt),
                Some(b.number),
                "abbr_pt {}",
                b.abbrev_pt
            );
        }
        assert!(
            map.len() > 300,
            "mapa de aliases pequeno demais: {}",
            map.len()
        );
    }

    #[test]
    fn portuguese_job_vs_john_disambiguation() {
        // `Jó` (com acento) = Job (18); `Jo` (sem acento) = João (43).
        assert_eq!(book_number("Jó"), Some(18));
        assert_eq!(book_number("Job"), Some(18));
        assert_eq!(book_number("Jo"), Some(43));
        assert_eq!(book_number("João"), Some(43));
        assert_eq!(book_number("joao"), Some(43));
        // `Jn` = Jonas (32), não João.
        assert_eq!(book_number("Jn"), Some(32));
    }

    #[test]
    fn roman_numeral_books() {
        assert_eq!(book_number("II Coríntios"), Some(47));
        assert_eq!(book_number("I John"), Some(62));
        assert_eq!(book_number("III João"), Some(64));
    }

    #[test]
    fn handles_decomposed_utf8_nfd_input() {
        // "Coríntios" em NFD: 'i' seguido de acento agudo combinante (U+0301).
        let nfd = "1 Cori\u{0301}ntios 13.4";
        assert_eq!(
            parse_reference(nfd).unwrap(),
            r(46, 13, VerseRange::Single(4))
        );
        // "João" decomposto (a + til combinante U+0303).
        assert_eq!(book_number("Joa\u{0303}o"), Some(43));
    }

    // ---- ≥20 formatos válidos ----

    #[test]
    fn valid_single_verse_pt() {
        assert_eq!(
            parse_reference("Jo 3.16").unwrap(),
            r(43, 3, VerseRange::Single(16))
        );
    }
    #[test]
    fn valid_single_verse_en_colon() {
        assert_eq!(
            parse_reference("John 3:16").unwrap(),
            r(43, 3, VerseRange::Single(16))
        );
    }
    #[test]
    fn valid_range_pt() {
        assert_eq!(
            parse_reference("Gn 1.1-3").unwrap(),
            r(1, 1, VerseRange::Range { start: 1, end: 3 })
        );
    }
    #[test]
    fn valid_whole_chapter() {
        assert_eq!(
            parse_reference("Sl 23").unwrap(),
            r(19, 23, VerseRange::WholeChapter)
        );
    }
    #[test]
    fn valid_numbered_book_no_space() {
        assert_eq!(
            parse_reference("1Co 13.4-7").unwrap(),
            r(46, 13, VerseRange::Range { start: 4, end: 7 })
        );
    }
    #[test]
    fn valid_numbered_book_with_space() {
        assert_eq!(
            parse_reference("1 Coríntios 13.4").unwrap(),
            r(46, 13, VerseRange::Single(4))
        );
    }
    #[test]
    fn valid_full_name_en() {
        assert_eq!(
            parse_reference("Genesis 1:1").unwrap(),
            r(1, 1, VerseRange::Single(1))
        );
    }
    #[test]
    fn valid_full_name_pt_accented() {
        assert_eq!(
            parse_reference("Gênesis 1.1").unwrap(),
            r(1, 1, VerseRange::Single(1))
        );
    }
    #[test]
    fn valid_multiword_book() {
        assert_eq!(
            parse_reference("Song of Solomon 2:1").unwrap(),
            r(22, 2, VerseRange::Single(1))
        );
    }
    #[test]
    fn valid_psalm_long_chapter() {
        assert_eq!(
            parse_reference("Salmos 119.105").unwrap(),
            r(19, 119, VerseRange::Single(105))
        );
    }
    #[test]
    fn valid_revelation_pt() {
        assert_eq!(
            parse_reference("Ap 22.21").unwrap(),
            r(66, 22, VerseRange::Single(21))
        );
    }
    #[test]
    fn valid_extra_spaces() {
        assert_eq!(
            parse_reference("  Mt   5 : 9 ").unwrap(),
            r(40, 5, VerseRange::Single(9))
        );
    }
    #[test]
    fn valid_colon_range() {
        assert_eq!(
            parse_reference("Romanos 8:38-39").unwrap(),
            r(45, 8, VerseRange::Range { start: 38, end: 39 })
        );
    }
    #[test]
    fn valid_lowercase_input() {
        assert_eq!(
            parse_reference("joão 1.1").unwrap(),
            r(43, 1, VerseRange::Single(1))
        );
    }
    #[test]
    fn valid_abbrev_with_dot() {
        assert_eq!(
            parse_reference("Gen. 2:7").unwrap(),
            r(1, 2, VerseRange::Single(7))
        );
    }
    #[test]
    fn valid_job_accented() {
        assert_eq!(
            parse_reference("Jó 1.1").unwrap(),
            r(18, 1, VerseRange::Single(1))
        );
    }
    #[test]
    fn valid_jonah_abbrev() {
        assert_eq!(
            parse_reference("Jn 2.1").unwrap(),
            r(32, 2, VerseRange::Single(1))
        );
    }
    #[test]
    fn valid_third_john_whole_chapter() {
        assert_eq!(
            parse_reference("3Jo 1").unwrap(),
            r(64, 1, VerseRange::WholeChapter)
        );
    }
    #[test]
    fn valid_dot_separator_en_name() {
        assert_eq!(
            parse_reference("Matthew 6.33").unwrap(),
            r(40, 6, VerseRange::Single(33))
        );
    }
    #[test]
    fn valid_collapsing_equal_range_to_single() {
        assert_eq!(
            parse_reference("Tg 2.24-24").unwrap(),
            r(59, 2, VerseRange::Single(24))
        );
    }
    #[test]
    fn valid_psalm_word_singular() {
        assert_eq!(
            parse_reference("Salmo 23.1").unwrap(),
            r(19, 23, VerseRange::Single(1))
        );
    }
    #[test]
    fn valid_first_peter() {
        assert_eq!(
            parse_reference("1Pe 5.7").unwrap(),
            r(60, 5, VerseRange::Single(7))
        );
    }

    #[test]
    fn valid_reference_list() {
        let refs = parse_references("Jo 3.16; Rm 8.28; Sl 23").unwrap();
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0], r(43, 3, VerseRange::Single(16)));
        assert_eq!(refs[1], r(45, 8, VerseRange::Single(28)));
        assert_eq!(refs[2], r(19, 23, VerseRange::WholeChapter));
    }

    // ---- ≥8 formatos inválidos ----

    #[test]
    fn invalid_empty() {
        assert_eq!(parse_reference(""), Err(ReferenceError::Empty));
    }
    #[test]
    fn invalid_whitespace_only() {
        assert_eq!(parse_reference("   "), Err(ReferenceError::Empty));
    }
    #[test]
    fn invalid_unknown_book() {
        assert!(matches!(
            parse_reference("Xyz 3.16"),
            Err(ReferenceError::UnknownBook(_))
        ));
    }
    #[test]
    fn invalid_missing_chapter() {
        assert!(matches!(
            parse_reference("John"),
            Err(ReferenceError::Malformed(_))
        ));
    }
    #[test]
    fn invalid_dangling_separator() {
        assert!(matches!(
            parse_reference("John 3:"),
            Err(ReferenceError::Malformed(_))
        ));
    }
    #[test]
    fn invalid_non_numeric_chapter() {
        assert!(matches!(
            parse_reference("John abc"),
            Err(ReferenceError::Malformed(_))
        ));
    }
    #[test]
    fn invalid_missing_book() {
        assert!(matches!(
            parse_reference("3:16"),
            Err(ReferenceError::Malformed(_))
        ));
    }
    #[test]
    fn invalid_chapter_zero() {
        assert!(matches!(
            parse_reference("John 0:16"),
            Err(ReferenceError::InvalidNumber(_))
        ));
    }
    #[test]
    fn invalid_verse_zero() {
        assert!(matches!(
            parse_reference("John 3:0"),
            Err(ReferenceError::InvalidNumber(_))
        ));
    }
    #[test]
    fn invalid_inverted_range() {
        assert_eq!(
            parse_reference("John 3:16-12"),
            Err(ReferenceError::RangeOrder { start: 16, end: 12 })
        );
    }
    #[test]
    fn invalid_trailing_junk() {
        assert!(matches!(
            parse_reference("John 3:16xyz"),
            Err(ReferenceError::Malformed(_))
        ));
    }
    #[test]
    fn invalid_dangling_range() {
        assert!(matches!(
            parse_reference("Gn 1.1-"),
            Err(ReferenceError::Malformed(_))
        ));
    }

    // ---- formatação ----

    #[test]
    fn format_pt_and_en() {
        let single = r(43, 3, VerseRange::Single(16));
        assert_eq!(format_reference(&single, Lang::Pt), "João 3.16");
        assert_eq!(format_reference(&single, Lang::En), "John 3:16");
        let range = r(1, 1, VerseRange::Range { start: 1, end: 3 });
        assert_eq!(format_reference(&range, Lang::Pt), "Gênesis 1.1-3");
        assert_eq!(format_reference(&range, Lang::En), "Genesis 1:1-3");
        let whole = r(19, 23, VerseRange::WholeChapter);
        assert_eq!(format_reference(&whole, Lang::Pt), "Salmos 23");
    }

    // ---- varredura de referências em prosa ----

    #[test]
    fn scan_finds_references_in_prose() {
        let refs = scan_references("Como diz João 3:16 e Romanos 5.8, veja também Salmo 23.");
        let got: Vec<Reference> = refs.iter().map(|s| s.reference).collect();
        assert!(got.contains(&r(43, 3, VerseRange::Single(16))), "{got:?}");
        assert!(got.contains(&r(45, 5, VerseRange::Single(8))), "{got:?}");
        assert!(
            got.contains(&r(19, 23, VerseRange::WholeChapter)),
            "{got:?}"
        );
    }

    #[test]
    fn scan_handles_ranges_and_numbered_books() {
        let refs = scan_references("Leia 1 Coríntios 13:4-7 com atenção.");
        assert_eq!(refs.len(), 1, "{refs:?}");
        assert_eq!(
            refs[0].reference,
            r(46, 13, VerseRange::Range { start: 4, end: 7 })
        );
        // O numeral inicial não vira um capítulo solto.
        assert!(!refs.iter().any(|s| s.reference.chapter == 1));
    }

    #[test]
    fn scan_ignores_bare_abbrev_and_out_of_range() {
        // "am 7" (abreviação sem versículo) é ruído; capítulo fora do intervalo
        // do livro é descartado; só a referência plena sobrevive.
        let refs = scan_references("eu am 7 anos; João 999 não existe; mas João 3:16 sim.");
        assert!(refs
            .iter()
            .any(|s| s.reference == r(43, 3, VerseRange::Single(16))));
        assert!(!refs.iter().any(|s| s.reference.chapter == 999));
        assert!(!refs.iter().any(|s| s.reference.chapter == 7));
    }

    #[test]
    fn scan_ranges_point_to_the_matched_text() {
        let text = "ver João 3:16 aqui";
        let refs = scan_references(text);
        assert_eq!(refs.len(), 1);
        assert_eq!(&text[refs[0].range.clone()], "João 3:16");
    }
}
