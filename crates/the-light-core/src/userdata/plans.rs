//! Planos de leitura: geração (anual/NT/evangelhos), progresso e persistência.
//!
//! Um plano é uma sequência de dias, cada dia com uma lista de referências
//! (capítulos inteiros). O "dia de hoje" é derivado da **data** (injeção via
//! parâmetro → testável), e o progresso (dias concluídos) é persistido em
//! `reading-plans/active.json`.

#[cfg(feature = "embedded")]
use std::path::PathBuf;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[cfg(feature = "embedded")]
use super::Result;
use crate::model::Reference;
use crate::reference::chapters_in_book;

/// Um plano de leitura concreto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Slug do plano.
    pub id: &'static str,
    /// Nome legível.
    pub name: &'static str,
    /// Leituras por dia (cada dia: capítulos inteiros).
    pub days: Vec<Vec<Reference>>,
}

impl Plan {
    /// Número de dias do plano.
    pub fn len(&self) -> usize {
        self.days.len()
    }

    /// `true` se o plano não tem dias.
    pub fn is_empty(&self) -> bool {
        self.days.is_empty()
    }

    /// Leituras de um dia (0-based).
    pub fn reading(&self, day: usize) -> &[Reference] {
        self.days.get(day).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Planos disponíveis: `(id, nome, intervalo de livros, dias)`.
const CATALOG: &[(&str, &str, u8, u8, usize)] = &[
    ("annual", "Bíblia em 1 ano", 1, 66, 365),
    ("nt", "Novo Testamento em 90 dias", 40, 66, 90),
    ("gospels", "Evangelhos em 30 dias", 40, 43, 30),
];

/// Lista os planos disponíveis como `(id, nome)`.
pub fn available_plans() -> Vec<(&'static str, &'static str)> {
    CATALOG.iter().map(|(id, name, ..)| (*id, *name)).collect()
}

/// Constrói um plano pelo id, ou `None` se desconhecido.
pub fn plan_by_id(id: &str) -> Option<Plan> {
    let &(id, name, first, last, days) = CATALOG.iter().find(|(i, ..)| *i == id)?;
    let chapters = chapters_for_books(first, last);
    Some(Plan {
        id,
        name,
        days: chunk(chapters, days),
    })
}

/// Referências (capítulo inteiro) de todos os capítulos dos livros `first..=last`.
fn chapters_for_books(first: u8, last: u8) -> Vec<Reference> {
    let mut out = Vec::new();
    for book in first..=last {
        for chapter in 1..=chapters_in_book(book) {
            out.push(Reference::whole_chapter(book, chapter));
        }
    }
    out
}

/// Divide `items` em `days` blocos o mais uniformemente possível.
fn chunk(items: Vec<Reference>, days: usize) -> Vec<Vec<Reference>> {
    if days == 0 {
        return Vec::new();
    }
    let base = items.len() / days;
    let rem = items.len() % days;
    let mut out = Vec::with_capacity(days);
    let mut idx = 0;
    for d in 0..days {
        let take = base + usize::from(d < rem);
        out.push(items[idx..idx + take].to_vec());
        idx += take;
    }
    out
}

/// Progresso de um plano ativo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanProgress {
    /// Slug do plano.
    pub plan_id: String,
    /// Data de início (dia 1).
    pub start_date: NaiveDate,
    /// Quantidade de dias marcados como concluídos.
    pub completed: u32,
}

impl PlanProgress {
    /// Índice (0-based) do dia correspondente a `date`, limitado a `[0, len-1]`.
    pub fn day_index_for(&self, date: NaiveDate, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        let diff = (date - self.start_date).num_days();
        diff.clamp(0, (len - 1) as i64) as usize
    }
}

/// Persistência do plano ativo (`reading-plans/active.json`). Usa fs (`directories`
/// via `super::reading_plans_dir`, `crate::util::atomic_write`) → só `embedded`. A web
/// persiste o progresso em OPFS app-side (a GERAÇÃO acima é pura/wasm-safe).
#[cfg(feature = "embedded")]
pub struct PlanStore {
    path: PathBuf,
}

#[cfg(feature = "embedded")]
impl PlanStore {
    /// Cria um store ligado ao arquivo dado.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        PlanStore { path: path.into() }
    }

    /// Store no caminho padrão (`reading-plans/active.json`).
    pub fn open_default() -> Result<Self> {
        Ok(PlanStore::new(
            super::reading_plans_dir()?.join("active.json"),
        ))
    }

    /// Lê o progresso ativo, se houver.
    pub fn load(&self) -> Result<Option<PlanProgress>> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Grava o progresso, de forma atômica.
    pub fn save(&self, progress: &PlanProgress) -> Result<()> {
        let json = serde_json::to_string_pretty(progress)?;
        crate::util::atomic_write(&self.path, json.as_bytes())?;
        Ok(())
    }

    /// Remove o plano ativo. Devolve `true` se havia um.
    pub fn clear(&self) -> Result<bool> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VerseRange;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn annual_plan_covers_whole_bible_in_365_days() {
        let plan = plan_by_id("annual").unwrap();
        assert_eq!(plan.len(), 365);
        let total: usize = plan.days.iter().map(Vec::len).sum();
        assert_eq!(total, 1189, "total de capítulos");
        // Cada item é capítulo inteiro.
        assert!(matches!(plan.days[0][0].verses, VerseRange::WholeChapter));
        // Começa em Gênesis 1.
        assert_eq!(plan.days[0][0].book, 1);
        assert_eq!(plan.days[0][0].chapter, 1);
    }

    #[test]
    fn nt_and_gospels_plans() {
        let nt = plan_by_id("nt").unwrap();
        assert_eq!(nt.len(), 90);
        assert_eq!(nt.days[0][0].book, 40); // Mateus
        let g = plan_by_id("gospels").unwrap();
        assert_eq!(g.len(), 30);
        let total: usize = g.days.iter().map(Vec::len).sum();
        assert_eq!(total, 28 + 16 + 24 + 21); // 89 capítulos dos 4 evangelhos
    }

    #[test]
    fn unknown_plan_is_none() {
        assert!(plan_by_id("nope").is_none());
    }

    #[test]
    fn chunk_distributes_evenly() {
        let items: Vec<Reference> = (1..=10).map(|c| Reference::whole_chapter(1, c)).collect();
        let days = chunk(items, 3);
        assert_eq!(days.iter().map(Vec::len).collect::<Vec<_>>(), vec![4, 3, 3]);
    }

    #[test]
    fn chunk_more_days_than_items_is_safe() {
        let items: Vec<Reference> = (1..=3).map(|c| Reference::whole_chapter(1, c)).collect();
        let days = chunk(items, 5);
        assert_eq!(days.len(), 5);
        assert_eq!(
            days.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![1, 1, 1, 0, 0]
        );
    }

    #[test]
    fn day_index_tracks_date_and_clamps() {
        let p = PlanProgress {
            plan_id: "annual".into(),
            start_date: date(2026, 1, 1),
            completed: 0,
        };
        assert_eq!(p.day_index_for(date(2026, 1, 1), 365), 0);
        assert_eq!(p.day_index_for(date(2026, 1, 11), 365), 10);
        // Antes do início → 0; muito depois → último dia.
        assert_eq!(p.day_index_for(date(2025, 12, 1), 365), 0);
        assert_eq!(p.day_index_for(date(2030, 1, 1), 365), 364);
    }

    #[test]
    #[cfg(feature = "embedded")]
    fn progress_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlanStore::new(dir.path().join("active.json"));
        assert!(store.load().unwrap().is_none());

        let p = PlanProgress {
            plan_id: "nt".into(),
            start_date: date(2026, 6, 16),
            completed: 5,
        };
        store.save(&p).unwrap();
        assert_eq!(store.load().unwrap(), Some(p));
        assert!(store.clear().unwrap());
        assert!(!store.clear().unwrap());
        assert!(store.load().unwrap().is_none());
    }
}
