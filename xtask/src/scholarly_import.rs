//! Subcomando `import-scholarly` do xtask — wrapper fino sobre
//! [`the_light_core::scholarly`]. A lógica (parsers, download, inserção,
//! enforcement de licença) vive no core para também servir à TUI/CLI do binário
//! enviado; aqui só fazemos o parsing de argumentos e o relato no stdout.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use the_light_core::scholarly;
use the_light_core::store::Store;

/// Ponto de entrada do subcomando `import-scholarly`.
pub fn run(args: &[String]) -> Result<()> {
    let mut versions: Vec<String> = Vec::new();
    let mut db_path: Option<PathBuf> = None;
    let mut seed_dir = PathBuf::from("data/seed/scholarly");
    let mut force = false;
    let mut offline = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--version" => {
                let v = it
                    .next()
                    .context("--version requer uma lista (ex.: tahot,tagnt)")?;
                versions = v
                    .split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "--db" => db_path = Some(PathBuf::from(it.next().context("--db requer caminho")?)),
            "--seed-dir" => {
                seed_dir = PathBuf::from(it.next().context("--seed-dir requer caminho")?)
            }
            "--force" => force = true,
            "--offline" => offline = true,
            other => bail!("argumento desconhecido para `import-scholarly`: {other}"),
        }
    }
    if versions.is_empty() {
        versions = scholarly::default_datasets();
    }

    let mut store = match &db_path {
        Some(p) => Store::open(p),
        None => Store::open_default(),
    }
    .context("abrindo o banco")?;

    let mut progress = |msg: &str| println!("  {msg}");
    let summary = scholarly::import(
        store.conn_mut(),
        &versions,
        &seed_dir,
        offline,
        force,
        &mut progress,
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let total: usize = summary.iter().map(|(_, n)| n).sum();
    println!(
        "✓ {total} registros importados em {} conjunto(s) (STEPBible, CC BY 4.0).",
        summary.len()
    );
    println!(
        "\nAtribuição obrigatória (gravada em scholarly_sources): {}",
        scholarly::ATTRIBUTION
    );
    Ok(())
}
