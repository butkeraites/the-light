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

## ADR-0005 — Busca acento-insensível via FTS5, sem coluna `text_fold` · 2026-06-15
**Contexto:** a pesquisa de datasets sugeriu uma coluna `text_fold` (NFD + strip de
marcas) para busca PT sem acento.
**Decisão:** usar o `verses_fts` com `tokenize='unicode61 remove_diacritics 2'`, que
já dobra acentos nos dois lados (índice e query). Sem coluna extra nem dep de
`unicode-normalization` na Fase 0.
**Consequência:** schema do plano mantido; `graca`↔`graça`, `ceus`↔`céus` verificados.

## ADR-0006 — Códigos de saída do `read` · 2026-06-15
**Contexto:** a CLI precisa sinalizar erro de uso vs. nada encontrado.
**Decisão:** `0` sucesso; `1` referência válida mas sem texto/versão; `2` referência
inválida (erro de parsing). `EmbeddedSource` implementa `BibleSource` lendo do SQLite.
**Consequência:** scripts podem distinguir os casos; testado via `assert_cmd`.

## ADR-0007 — Chaves de IA em `secrets.toml` (0600), não no keychain · 2026-06-16
**Contexto:** o SPEC pede armazenamento seguro das chaves BYOK (keychain quando
possível; senão arquivo restrito fora do git).
**Decisão:** guardar as chaves num `secrets.toml` separado do `config.toml`, no
diretório de config, com permissão `0600` (Unix) e no `.gitignore`. Caminho
sobrescrevível por `BIBLIA_SECRETS` (testes). O provedor ativo (não-secreto) fica
no `config.toml`. Integração com keychain do SO fica como evolução futura —
não é testável de forma determinística no ambiente atual e tocaria o keychain real.
**Consequência:** chaves nunca no git, nunca ecoadas; testes determinísticos via
`BIBLIA_SECRETS`. `KeyStore::list_providers` expõe só nomes, nunca valores.

## ADR-0008 — Anti-alucinação: texto citado vem do banco, não do LLM · 2026-06-16
**Contexto:** risco de alucinação em temas teológicos (SPEC §9).
**Decisão:** o `StudyResult` separa **texto citado** (extraído do banco local,
exato) de **interpretação** (saída do LLM). O prompt recebe a passagem e os
xrefs locais (RAG leve) e instrui a citar versículos e separar texto de
interpretação, sinalizando divergências entre tradições.
**Consequência:** a citação é sempre fiel; a lente é explícita; testes usam
`MockLlmProvider` (sem rede), provedores reais só em teste manual documentado.

## ADR-0004 — Licença do código `MIT OR Apache-2.0` · 2026-06-15
**Contexto:** o SPEC sugere MIT ou Apache-2.0; convenção do ecossistema Rust é dupla.
**Decisão:** `MIT OR Apache-2.0` para o código. Dados bíblicos seguem suas próprias
licenças (ver `DATA_SOURCES.md`); só versões de domínio público são embarcadas.
**Consequência:** máxima compatibilidade de reuso; fronteira legal de dados isolada.
