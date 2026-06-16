//! Subcomando `plan` — planos de leitura com progresso e export `.ics`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chrono::{Duration, Local, NaiveDate, Utc};
use clap::{Args, Subcommand};

use biblia_core::config::Config;
use biblia_core::model::Lang;
use biblia_core::reference::format_reference;
use biblia_core::userdata::plans::{available_plans, plan_by_id, Plan, PlanProgress};
use biblia_core::userdata::PlanStore;

/// Argumentos do subcomando `plan`.
#[derive(Args)]
pub struct PlanArgs {
    #[command(subcommand)]
    action: PlanAction,
}

#[derive(Subcommand)]
enum PlanAction {
    /// Lista os planos disponíveis.
    List,
    /// Inicia um plano: `plan start annual [--year 2026 | --date 2026-06-16]`.
    Start {
        /// Slug do plano (ex.: annual, nt, gospels).
        plan: String,
        /// Ano de início (dia 1 = 1º de janeiro).
        #[arg(long)]
        year: Option<i32>,
        /// Data de início (YYYY-MM-DD); padrão: hoje.
        #[arg(long)]
        date: Option<String>,
    },
    /// Mostra a leitura do dia.
    Today {
        /// Data de referência (YYYY-MM-DD); padrão: hoje.
        #[arg(long)]
        date: Option<String>,
    },
    /// Mostra o progresso do plano ativo.
    Status {
        /// Data de referência (YYYY-MM-DD); padrão: hoje.
        #[arg(long)]
        date: Option<String>,
    },
    /// Marca mais um dia como concluído.
    Mark,
    /// Encerra o plano ativo.
    Reset,
    /// Exporta o plano como calendário `.ics`.
    Ics {
        /// Arquivo de saída; omitir imprime no stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

const EXIT_OK: u8 = 0;
const EXIT_NOT_FOUND: u8 = 1;
const EXIT_USAGE: u8 = 2;

/// Executa o comando `plan`.
pub fn run(args: PlanArgs) -> ExitCode {
    match args.action {
        PlanAction::List => list(),
        PlanAction::Start { plan, year, date } => start(&plan, year, date.as_deref()),
        PlanAction::Today { date } => today(date.as_deref()),
        PlanAction::Status { date } => status(date.as_deref()),
        PlanAction::Mark => mark(),
        PlanAction::Reset => reset(),
        PlanAction::Ics { output } => ics(output.as_deref()),
    }
}

fn lang() -> Lang {
    Config::load().unwrap_or_default().language
}

fn store() -> std::result::Result<PlanStore, ExitCode> {
    PlanStore::open_default().map_err(|e| {
        eprintln!("Erro ao acessar planos: {e}");
        ExitCode::from(EXIT_NOT_FOUND)
    })
}

fn parse_date(s: &str) -> std::result::Result<NaiveDate, ExitCode> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| {
        eprintln!("Data inválida: `{s}` (use YYYY-MM-DD).");
        ExitCode::from(EXIT_USAGE)
    })
}

/// Data de referência: `--date`, senão hoje (local).
fn ref_date(date: Option<&str>) -> std::result::Result<NaiveDate, ExitCode> {
    match date {
        Some(s) => parse_date(s),
        None => Ok(Local::now().date_naive()),
    }
}

/// Formata as referências de um dia (`Gênesis 1; Gênesis 2; ...`).
fn format_readings(plan: &Plan, day: usize, lang: Lang) -> String {
    plan.reading(day)
        .iter()
        .map(|r| format_reference(r, lang))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Carrega o progresso ativo e o respectivo plano.
fn active() -> std::result::Result<(PlanStore, PlanProgress, Plan), ExitCode> {
    let st = store()?;
    let progress = match st.load() {
        Ok(Some(p)) => p,
        Ok(None) => {
            eprintln!("Nenhum plano ativo. Use `biblia plan start <id>`.");
            return Err(ExitCode::from(EXIT_NOT_FOUND));
        }
        Err(e) => {
            eprintln!("Erro ao ler o plano: {e}");
            return Err(ExitCode::from(EXIT_NOT_FOUND));
        }
    };
    let Some(plan) = plan_by_id(&progress.plan_id) else {
        eprintln!("Plano `{}` não existe mais.", progress.plan_id);
        return Err(ExitCode::from(EXIT_NOT_FOUND));
    };
    Ok((st, progress, plan))
}

fn list() -> ExitCode {
    println!("Planos disponíveis:");
    for (id, name) in available_plans() {
        let days = plan_by_id(id).map(|p| p.len()).unwrap_or(0);
        println!("  {id:<10} {name} ({days} dias)");
    }
    ExitCode::from(EXIT_OK)
}

fn start(plan_id: &str, year: Option<i32>, date: Option<&str>) -> ExitCode {
    let Some(plan) = plan_by_id(plan_id) else {
        eprintln!(
            "Plano desconhecido: `{plan_id}`. Disponíveis: {}",
            available_plans()
                .iter()
                .map(|(i, _)| *i)
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::from(EXIT_USAGE);
    };
    let start_date = match (date, year) {
        (Some(s), _) => match parse_date(s) {
            Ok(d) => d,
            Err(c) => return c,
        },
        (None, Some(y)) => match NaiveDate::from_ymd_opt(y, 1, 1) {
            Some(d) => d,
            None => {
                eprintln!("Ano inválido: {y}.");
                return ExitCode::from(EXIT_USAGE);
            }
        },
        (None, None) => Local::now().date_naive(),
    };

    let st = match store() {
        Ok(s) => s,
        Err(c) => return c,
    };
    let progress = PlanProgress {
        plan_id: plan.id.to_string(),
        start_date,
        completed: 0,
    };
    if let Err(e) = st.save(&progress) {
        eprintln!("Erro ao gravar o plano: {e}");
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    println!(
        "Plano iniciado: {} ({} dias), começando em {}.",
        plan.name,
        plan.len(),
        start_date
    );
    ExitCode::from(EXIT_OK)
}

fn today(date: Option<&str>) -> ExitCode {
    let date = match ref_date(date) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let (_st, progress, plan) = match active() {
        Ok(t) => t,
        Err(c) => return c,
    };
    let day = progress.day_index_for(date, plan.len());
    println!("{} — dia {}/{}", plan.name, day + 1, plan.len());
    let readings = format_readings(&plan, day, lang());
    if readings.is_empty() {
        println!("  (sem leitura)");
    } else {
        println!("  {readings}");
    }
    ExitCode::from(EXIT_OK)
}

fn status(date: Option<&str>) -> ExitCode {
    let date = match ref_date(date) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let (_st, progress, plan) = match active() {
        Ok(t) => t,
        Err(c) => return c,
    };
    let day = progress.day_index_for(date, plan.len()) + 1;
    let pct = if plan.is_empty() {
        0.0
    } else {
        progress.completed as f64 / plan.len() as f64 * 100.0
    };
    println!("Plano: {} ({})", plan.name, plan.id);
    println!("Início: {}", progress.start_date);
    println!("Hoje: dia {day}/{} (pela data)", plan.len());
    println!(
        "Concluídos: {}/{} ({:.0}%)",
        progress.completed,
        plan.len(),
        pct
    );
    ExitCode::from(EXIT_OK)
}

fn mark() -> ExitCode {
    let (st, mut progress, plan) = match active() {
        Ok(t) => t,
        Err(c) => return c,
    };
    progress.completed = (progress.completed + 1).min(plan.len() as u32);
    if let Err(e) = st.save(&progress) {
        eprintln!("Erro ao gravar o progresso: {e}");
        return ExitCode::from(EXIT_NOT_FOUND);
    }
    println!("Concluídos: {}/{}", progress.completed, plan.len());
    ExitCode::from(EXIT_OK)
}

fn reset() -> ExitCode {
    let st = match store() {
        Ok(s) => s,
        Err(c) => return c,
    };
    match st.clear() {
        Ok(true) => {
            println!("Plano encerrado.");
            ExitCode::from(EXIT_OK)
        }
        Ok(false) => {
            println!("Nenhum plano ativo.");
            ExitCode::from(EXIT_NOT_FOUND)
        }
        Err(e) => {
            eprintln!("Erro ao encerrar o plano: {e}");
            ExitCode::from(EXIT_NOT_FOUND)
        }
    }
}

/// Escapa texto para um campo de iCalendar (RFC 5545).
fn ics_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn ics(output: Option<&Path>) -> ExitCode {
    let (_st, progress, plan) = match active() {
        Ok(t) => t,
        Err(c) => return c,
    };
    let lang = lang();
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();

    let mut ics = String::new();
    ics.push_str("BEGIN:VCALENDAR\r\n");
    ics.push_str("VERSION:2.0\r\n");
    ics.push_str("PRODID:-//Biblia CLI//Reading Plan//PT\r\n");
    ics.push_str("CALSCALE:GREGORIAN\r\n");
    for (i, _) in plan.days.iter().enumerate() {
        let day_date = progress.start_date + Duration::days(i as i64);
        let next = day_date + Duration::days(1);
        let summary = ics_escape(&format!(
            "Leitura ({}/{}) : {}",
            i + 1,
            plan.len(),
            format_readings(&plan, i, lang)
        ));
        ics.push_str("BEGIN:VEVENT\r\n");
        ics.push_str(&format!("UID:biblia-{}-{}@biblia-cli\r\n", plan.id, i + 1));
        ics.push_str(&format!("DTSTAMP:{stamp}\r\n"));
        ics.push_str(&format!(
            "DTSTART;VALUE=DATE:{}\r\n",
            day_date.format("%Y%m%d")
        ));
        ics.push_str(&format!("DTEND;VALUE=DATE:{}\r\n", next.format("%Y%m%d")));
        ics.push_str(&format!("SUMMARY:{summary}\r\n"));
        ics.push_str("END:VEVENT\r\n");
    }
    ics.push_str("END:VCALENDAR\r\n");

    match output {
        Some(path) => {
            if let Err(e) = biblia_core::util::atomic_write(path, ics.as_bytes()) {
                eprintln!("Erro ao gravar {}: {e}", path.display());
                return ExitCode::from(EXIT_NOT_FOUND);
            }
            println!(
                "Calendário exportado para {} ({} eventos).",
                path.display(),
                plan.len()
            );
        }
        None => print!("{ics}"),
    }
    ExitCode::from(EXIT_OK)
}
