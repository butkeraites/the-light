//! Renderização de passagens para o terminal.
//!
//! Funções puras (sem I/O) e testáveis: produzem `String`. A escolha entre
//! colunas lado a lado e blocos intercalados é feita pelo chamador a partir da
//! largura disponível.

use std::collections::BTreeSet;

/// Largura mínima útil de uma coluna; abaixo disso caímos para blocos.
pub const MIN_COL_WIDTH: usize = 18;
/// Separador visual entre colunas (3 colunas visíveis).
const SEP: &str = " │ ";

/// Uma versão a ser renderizada: rótulo, referência já formatada e versículos.
pub struct VersionColumn {
    /// Abreviação da versão, ex.: `"KJV"`.
    pub label: String,
    /// Referência formatada no idioma da versão, ex.: `"John 3:16"`.
    pub reference: String,
    /// Versículos `(número, texto)`, em ordem.
    pub verses: Vec<(u16, String)>,
}

/// Número de caracteres (aproxima colunas do terminal para PT/EN).
fn width_of(s: &str) -> usize {
    s.chars().count()
}

/// Largura do medianiz (números de versículo) para o conjunto dado.
fn gutter_width(numbers: &[u16]) -> usize {
    numbers
        .iter()
        .map(|n| n.to_string().len())
        .max()
        .unwrap_or(2)
        .max(2)
}

/// Trunca (com `…`) ou preenche `s` para exatamente `w` caracteres.
fn fit(s: &str, w: usize) -> String {
    let len = width_of(s);
    if len == w {
        s.to_string()
    } else if len < w {
        let mut out = s.to_string();
        out.extend(std::iter::repeat(' ').take(w - len));
        out
    } else if w == 0 {
        String::new()
    } else {
        let mut out: String = s.chars().take(w - 1).collect();
        out.push('…');
        out
    }
}

/// Quebra `text` em linhas de no máximo `w` caracteres (gulosa, por palavra).
/// Palavras maiores que `w` são quebradas à força.
fn wrap_text(text: &str, w: usize) -> Vec<String> {
    if w == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;

    for word in text.split_whitespace() {
        let wl = width_of(word);
        if wl > w {
            // Palavra maior que a coluna: descarrega a linha atual e quebra a palavra.
            if current_len > 0 {
                lines.push(std::mem::take(&mut current));
                current_len = 0;
            }
            let mut chunk = String::new();
            let mut chunk_len = 0;
            for ch in word.chars() {
                if chunk_len == w {
                    lines.push(std::mem::take(&mut chunk));
                    chunk_len = 0;
                }
                chunk.push(ch);
                chunk_len += 1;
            }
            if chunk_len > 0 {
                current = chunk;
                current_len = chunk_len;
            }
            continue;
        }
        let extra = if current_len == 0 { wl } else { wl + 1 };
        if current_len + extra > w {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_len = wl;
        } else {
            if current_len > 0 {
                current.push(' ');
                current_len += 1;
            }
            current.push_str(word);
            current_len += wl;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Renderiza uma única versão (número de versículo + texto).
pub fn render_single(col: &VersionColumn) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} ({})\n", col.reference, col.label));
    if col.verses.is_empty() {
        out.push_str("  (nenhum versículo encontrado)\n");
        return out;
    }
    let nums: Vec<u16> = col.verses.iter().map(|(n, _)| *n).collect();
    let gw = gutter_width(&nums);
    for (n, text) in &col.verses {
        out.push_str(&format!("  {n:>gw$}  {text}\n"));
    }
    out
}

/// Renderiza várias versões em blocos intercalados por versículo (sem colunas).
/// Robusto para pipes e terminais estreitos.
pub fn render_interleaved(cols: &[VersionColumn]) -> String {
    let mut out = String::new();
    // Cabeçalho: "ref1 (A) | ref2 (B)".
    let header = cols
        .iter()
        .map(|c| format!("{} ({})", c.reference, c.label))
        .collect::<Vec<_>>()
        .join("  |  ");
    out.push_str(&header);
    out.push('\n');

    let nums = union_numbers(cols);
    if nums.is_empty() {
        out.push_str("  (nenhum versículo encontrado)\n");
        return out;
    }
    let gw = gutter_width(&nums);
    let label_w = cols.iter().map(|c| width_of(&c.label)).max().unwrap_or(0);

    for n in &nums {
        out.push('\n');
        let mut first = true;
        for c in cols {
            let text = verse_text(c, *n).unwrap_or("");
            let num_field = if first {
                format!("{n:>gw$}")
            } else {
                " ".repeat(gw)
            };
            first = false;
            out.push_str(&format!("{num_field}  {:<label_w$}  {text}\n", c.label));
        }
    }
    out
}

/// Renderiza várias versões lado a lado, alinhadas por versículo, com quebra de
/// linha. Devolve `None` se a largura não comportar colunas mínimas.
pub fn render_columns(cols: &[VersionColumn], width: usize) -> Option<String> {
    let n = cols.len();
    if n == 0 {
        return Some(String::new());
    }
    let nums = union_numbers(cols);
    let gw = gutter_width(&nums);
    let prefix = gw + 2; // medianiz + 2 espaços
    let sep_total = width_of(SEP) * n.saturating_sub(1);
    let avail = width.saturating_sub(prefix + sep_total);
    let col_w = avail / n;
    if col_w < MIN_COL_WIDTH {
        return None;
    }

    let mut out = String::new();
    let mut push_line = |gutter: &str, cells: &[String]| {
        let mut line = String::from(gutter);
        line.push_str(&cells.join(SEP));
        out.push_str(line.trim_end());
        out.push('\n');
    };

    // Cabeçalho: rótulo + referência por coluna.
    let head_cells: Vec<String> = cols
        .iter()
        .map(|c| fit(&format!("{} — {}", c.label, c.reference), col_w))
        .collect();
    push_line(&" ".repeat(prefix), &head_cells);

    if nums.is_empty() {
        out.push_str(&" ".repeat(prefix));
        out.push_str("(nenhum versículo encontrado)\n");
        return Some(out);
    }

    for num in &nums {
        let wrapped: Vec<Vec<String>> = cols
            .iter()
            .map(|c| wrap_text(verse_text(c, *num).unwrap_or(""), col_w))
            .collect();
        let rows = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        for r in 0..rows {
            let gutter = if r == 0 {
                format!("{num:>gw$}  ")
            } else {
                " ".repeat(prefix)
            };
            let cells: Vec<String> = wrapped
                .iter()
                .map(|lines| fit(lines.get(r).map(String::as_str).unwrap_or(""), col_w))
                .collect();
            push_line(&gutter, &cells);
        }
    }
    Some(out)
}

fn union_numbers(cols: &[VersionColumn]) -> Vec<u16> {
    let set: BTreeSet<u16> = cols
        .iter()
        .flat_map(|c| c.verses.iter().map(|(n, _)| *n))
        .collect();
    set.into_iter().collect()
}

fn verse_text(col: &VersionColumn, number: u16) -> Option<&str> {
    col.verses
        .iter()
        .find(|(n, _)| *n == number)
        .map(|(_, t)| t.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(label: &str, reference: &str, verses: &[(u16, &str)]) -> VersionColumn {
        VersionColumn {
            label: label.to_string(),
            reference: reference.to_string(),
            verses: verses.iter().map(|(n, t)| (*n, t.to_string())).collect(),
        }
    }

    #[test]
    fn wrap_breaks_on_words() {
        assert_eq!(wrap_text("a b c d", 3), vec!["a b", "c d"]);
        assert_eq!(wrap_text("hello world", 5), vec!["hello", "world"]);
    }

    #[test]
    fn wrap_hard_breaks_long_word() {
        assert_eq!(wrap_text("abcdefgh", 3), vec!["abc", "def", "gh"]);
    }

    #[test]
    fn fit_truncates_with_ellipsis_and_pads() {
        assert_eq!(fit("hello", 3), "he…");
        assert_eq!(fit("hi", 5), "hi   ");
        assert_eq!(fit("hey", 3), "hey");
    }

    #[test]
    fn single_version_format() {
        let c = col("KJV", "John 3:16", &[(16, "For God so loved the world")]);
        let out = render_single(&c);
        assert!(out.contains("John 3:16 (KJV)"));
        assert!(out.contains("  16  For God so loved the world"));
    }

    #[test]
    fn single_empty_shows_notice() {
        let c = col("KJV", "John 99:1", &[]);
        assert!(render_single(&c).contains("nenhum versículo encontrado"));
    }

    #[test]
    fn interleaved_aligns_by_verse() {
        let a = col(
            "KJV",
            "John 3:16-17",
            &[(16, "loved the world"), (17, "sent the Son")],
        );
        let b = col(
            "ALM",
            "João 3.16-17",
            &[(16, "amou o mundo"), (17, "enviou o Filho")],
        );
        let out = render_interleaved(&[a, b]);
        // Cada versículo agrupa as duas versões.
        let lines: Vec<&str> = out.lines().collect();
        // header + (blank + 2) * 2
        assert!(out.contains("loved the world"));
        assert!(out.contains("amou o mundo"));
        // A linha do verso 16 KJV vem antes da do verso 17.
        let pos16 = out.find("loved the world").unwrap();
        let pos17 = out.find("sent the Son").unwrap();
        assert!(pos16 < pos17);
        assert!(lines
            .iter()
            .any(|l| l.contains("KJV") && l.contains("loved the world")));
    }

    #[test]
    fn columns_side_by_side_wrap_and_align() {
        let a = col(
            "KJV",
            "John 3:16",
            &[(16, "For God so loved the world that he gave")],
        );
        let b = col(
            "ALM",
            "João 3.16",
            &[(16, "Porque Deus amou o mundo de tal maneira")],
        );
        let out = render_columns(&[a, b], 60).expect("largura suficiente");
        // Cabeçalho com os dois rótulos.
        assert!(out.contains("KJV"));
        assert!(out.contains("ALM"));
        // Número do versículo aparece no medianiz.
        assert!(out.lines().any(|l| l.trim_start().starts_with("16")));
        // Ambos os textos presentes (possivelmente quebrados).
        assert!(out.contains("For God so loved"));
        assert!(out.contains("Porque Deus amou"));
    }

    #[test]
    fn columns_returns_none_when_too_narrow() {
        let a = col("KJV", "John 3:16", &[(16, "x")]);
        let b = col("ALM", "João 3.16", &[(16, "y")]);
        // Largura 20 com 2 colunas → col_w < MIN_COL_WIDTH.
        assert!(render_columns(&[a, b], 20).is_none());
    }

    #[test]
    fn no_trailing_whitespace_in_columns() {
        let a = col("KJV", "John 3:16", &[(16, "short")]);
        let b = col("ALM", "João 3.16", &[(16, "curto")]);
        let out = render_columns(&[a, b], 60).unwrap();
        for line in out.lines() {
            assert_eq!(
                line,
                line.trim_end(),
                "linha com espaço à direita: {line:?}"
            );
        }
    }
}
