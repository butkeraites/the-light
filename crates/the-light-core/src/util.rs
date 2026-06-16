//! Utilitários internos compartilhados.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Grava `data` em `path` de forma **atômica**: escreve num arquivo temporário
/// no mesmo diretório, faz `sync` e renomeia por cima. Uma falha no meio nunca
/// deixa o arquivo de destino corrompido/truncado.
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => {
            std::fs::create_dir_all(p)?;
            p.to_path_buf()
        }
        _ => PathBuf::from("."),
    };
    let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
    tmp.write_all(data)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
