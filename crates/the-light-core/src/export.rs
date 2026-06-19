//! Exportação de documentos Markdown para arquivo: `.md` (escrita atômica) ou
//! `.pdf`/`.docx` via `pandoc`. Compartilhado pela CLI (`light export`,
//! `study --export`) e pela TUI (exportar um estudo). Vive no core para que
//! ambos os binários reutilizem uma única implementação.

use std::path::Path;

/// Slug de nome de arquivo: minúsculas, alfanumérico mantido, o resto vira `-`,
/// sem traços nas pontas (ex.: "Ef 2.8-9 (Presb.)" → "ef-2-8-9-presb").
pub fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Converte Markdown para `output` via pandoc (formato inferido pela extensão:
/// `.pdf`, `.docx`, …). Mensagem de erro amigável se o pandoc não existir.
pub fn run_pandoc(markdown: &str, output: &Path) -> Result<(), String> {
    use std::io::Write;
    let mut tmp = tempfile::Builder::new()
        .suffix(".md")
        .tempfile()
        .map_err(|e| format!("Erro ao criar arquivo temporário: {e}"))?;
    tmp.write_all(markdown.as_bytes())
        .map_err(|e| format!("Erro ao preparar Markdown: {e}"))?;
    match std::process::Command::new("pandoc")
        .arg(tmp.path())
        .arg("-o")
        .arg(output)
        .status()
    {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err("pandoc falhou ao gerar o documento.".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "`pandoc` não encontrado. Exporte em Markdown (.md) e converta você mesmo: \
             pandoc <arquivo>.md -o {}",
            output.display()
        )),
        Err(e) => Err(format!("Erro ao executar pandoc: {e}")),
    }
}

/// Exporta um documento Markdown: `.md`/`.markdown`/sem extensão → escrita
/// atômica; demais extensões → pandoc.
pub fn export_document(markdown: &str, output: &Path) -> Result<(), String> {
    match output.extension().and_then(|s| s.to_str()) {
        Some("md") | Some("markdown") | None => {
            crate::util::atomic_write(output, markdown.as_bytes())
                .map_err(|e| format!("Erro ao gravar {}: {e}", output.display()))
        }
        _ => run_pandoc(markdown, output),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowercases_and_dashes() {
        assert_eq!(slugify("Ef 2.8-9 (Presb.)"), "ef-2-8-9-presb");
        assert_eq!(slugify("João 3:16"), "jo-o-3-16");
        assert_eq!(slugify("---a---"), "a");
    }

    #[test]
    fn export_md_writes_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("estudo.md");
        export_document("# oi\n", &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# oi\n");
    }
}
