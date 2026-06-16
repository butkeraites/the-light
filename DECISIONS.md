# Decisões de Arquitetura (ADRs curtos)

> Um registro por decisão relevante. Formato: contexto → decisão → consequência.
> Datas em ISO (YYYY-MM-DD).

## ADR-0001 — Toolchain Rust via rustup · 2026-06-15
**Contexto:** a máquina de desenvolvimento não tinha Rust instalado; a Fase 0 exige
`cargo build/clippy/test`.
**Decisão:** instalar via `rustup` (toolchain `stable`, perfil default com clippy e
rustfmt). Fixado em `rust-toolchain.toml` (channel `stable`).
**Consequência:** ambiente reprodutível; `rust-version` mínima declarada como 1.80 no
workspace.

## ADR-0002 — Workspace com 3 crates + xtask · 2026-06-15
**Contexto:** o plano (§1) pede separar lógica testável de CLI e TUI.
**Decisão:** `crates/biblia-core` (lógica pura), `crates/biblia-cli` (binário `biblia`),
`crates/biblia-tui` (Fase 3) e `xtask` (importadores one-off). `resolver = "2"`.
**Consequência:** o núcleo não depende de `clap`/`ratatui`; testes de lógica isolados.

## ADR-0003 — Versões de dependências resolvidas pelo cargo · 2026-06-15
**Contexto:** o plano fixa majors (ex.: `rusqlite 0.31`, `ratatui 0.28`) mas pede
"confirmar versões mais recentes no momento da execução". Em jun/2026 várias avançaram.
**Decisão:** declarar dependências por crate com `cargo add` (resolve a versão estável
mais recente compatível) em vez de fixar minors do plano; registrar versões efetivas no
`Cargo.lock` (commitado, pois é uma aplicação).
**Consequência:** menos churn de versão; reprodutibilidade garantida pelo lockfile.

## ADR-0004 — Licença do código `MIT OR Apache-2.0` · 2026-06-15
**Contexto:** o SPEC sugere MIT ou Apache-2.0; convenção do ecossistema Rust é dupla.
**Decisão:** `MIT OR Apache-2.0` para o código. Dados bíblicos seguem suas próprias
licenças (ver `DATA_SOURCES.md`); só versões de domínio público são embarcadas.
**Consequência:** máxima compatibilidade de reuso; fronteira legal de dados isolada.
