//! Slug do arquivo de nota (`John_3.16.md`) — transformação **pura** sobre a
//! referência, independente de I/O. É `ai-pure` (wasm-safe): depende só de
//! `format_reference`/`parse_reference`. Fonte ÚNICA do formato para o nativo (fs,
//! `notes::NoteStore`) e para o web (OPFS, `userdata-fs.web.ts`) — ADR-0062.

use crate::model::{Lang, Reference};
use crate::reference::{format_reference, parse_reference};

/// Nome de arquivo (com `.md`) para uma referência: `format_reference(_, En)` com
/// `_` no lugar de espaço e `.` no lugar de `:`. Ex.: `John 3:16` → `John_3.16.md`.
pub fn slug(reference: &Reference) -> String {
    format!(
        "{}.md",
        format_reference(reference, Lang::En)
            .replace(' ', "_")
            .replace(':', ".")
    )
}

/// Recupera a referência a partir do `stem` (nome do arquivo SEM `.md`): desfaz o
/// `_`→espaço e delega a `parse_reference` (que aceita `.` como separador
/// capítulo:versículo). `None` se não parsear.
pub fn parse_slug(stem: &str) -> Option<Reference> {
    parse_reference(&stem.replace('_', " ")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_roundtrips_for_all_shapes() {
        for r in [
            "John 3:16",
            "Genesis 1:1-3",
            "Psalms 23",
            "1 John 2:1",
            "Song of Solomon 1:2",
        ] {
            let reference = parse_reference(r).unwrap();
            let name = slug(&reference);
            assert!(name.ends_with(".md"), "slug termina em .md: {name}");
            let stem = name.strip_suffix(".md").unwrap();
            assert_eq!(parse_slug(stem), Some(reference), "slug {name}");
        }
    }
}
