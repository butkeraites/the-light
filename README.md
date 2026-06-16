# Bíblia CLI

> Leitor de Bíblia hackeável para terminal, com estudo exegético assistido por IA
> (lente denominacional configurável), dados locais e modelo *bring-your-own-key*.

Status: **Fase 0 — Fundação** (em desenvolvimento). Veja [`SPEC.md`](SPEC.md) para a
visão e [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) para o roadmap executável.

## Princípios

1. **Offline-first** — o essencial funciona sem internet e sem IA.
2. **Bring-your-own-key (BYOK)** — a IA é opcional; o usuário paga seu próprio uso.
3. **Dados do usuário são do usuário** — notas/marcações em arquivos abertos, versionáveis.
4. **Licença em primeiro lugar** — só embarcamos versões livres (domínio público).
5. **Hackeável** — config em texto, fontes plugáveis, prompts editáveis.

## Estrutura do workspace

```
crates/
  biblia-core/   # lógica pura: modelo, parser de referências, store SQLite, fontes
  biblia-cli/    # binário `biblia` (clap)
  biblia-tui/    # interface ratatui (Fase 3)
xtask/           # tarefas de import de datasets livres (cargo run -p xtask)
data/            # datasets brutos (livres) — ver DATA_SOURCES.md
```

## Desenvolvimento

Requer Rust estável (instale via [rustup](https://rustup.rs)).

```sh
cargo build                       # compila o workspace
cargo test                        # roda os testes
cargo clippy -- -D warnings       # lint (sem warnings)
cargo fmt --check                 # formatação
```

## Documentos do projeto

- [`SPEC.md`](SPEC.md) — visão, arquitetura, decisões de design.
- [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) — roadmap por fases/tarefas.
- [`DATA_SOURCES.md`](DATA_SOURCES.md) — proveniência e licença de cada dataset.
- [`DECISIONS.md`](DECISIONS.md) — registros de decisão (ADRs curtos).
- [`PROGRESS.md`](PROGRESS.md) — log de execução tarefa a tarefa.

## Licença

Código sob `MIT OR Apache-2.0`. Os dados bíblicos seguem suas próprias licenças
(ver `DATA_SOURCES.md`); apenas versões de domínio público são embarcadas.
