# Bíblia CLI

> Leitor de Bíblia hackeável para terminal, com estudo exegético assistido por IA
> (lente denominacional configurável), dados locais e modelo *bring-your-own-key*.

Status: **Fase 3 — TUI concluída** (Marco 3, `v0.4.0`): interface `ratatui` completa
(navegação, leitura, painéis de estudo, busca interativa, tema) — sobre a base de
leitura/busca (Fase 1) e estudo pessoal (Fase 2). Veja [`SPEC.md`](SPEC.md) para a
visão e [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) para o roadmap.

## Uso rápido

```sh
# 1. Gerar o banco com as versões livres (uma vez):
cargo run -p xtask -- import --version kjv,alm1911 --db data/biblia.sqlite

# 2. Ler (PT ou EN, intervalos, capítulos, várias versões lado a lado):
cargo run -p biblia-cli -- read "John 3:16" --version kjv,alm1911 --db data/biblia.sqlite
cargo run -p biblia-cli -- read "Gn 1.1-3" --version alm1911 --db data/biblia.sqlite

# 3. Buscar (acento-insensível, ranqueado, com destaque e filtro de livro):
cargo run -p biblia-cli -- search "graça" --version alm1911 --book Romanos --db data/biblia.sqlite

# 4. Configurar preferências (versões padrão, idioma, tema):
cargo run -p biblia-cli -- config set versions kjv,alm1911
cargo run -p biblia-cli -- config list

# 5. Estudo pessoal: marcações, notas, referências cruzadas, export:
cargo run -p biblia-cli -- highlight add "Jo 3.16" --color yellow --tag salvação
cargo run -p biblia-cli -- note add "Jo 3.16" "Versículo **central**."
cargo run -p xtask -- import-xref --db data/biblia.sqlite   # referências cruzadas (TSK)
cargo run -p biblia-cli -- xref "Rm 3.23" --db data/biblia.sqlite
cargo run -p biblia-cli -- export notes --format md --output notas.md

# 6. Interface de terminal (TUI) completa:
cargo run -p biblia-cli -- tui --db data/biblia.sqlite
#   ↑↓ versículo · n/p capítulo · v versão · / buscar · g ir · x refs · t tema · q sair
```

> Dados do usuário (notas `.md`, `highlights.json`) vivem em arquivos abertos e
> versionáveis sob o diretório de dados do SO (ou `BIBLIA_DATA_DIR`).

> Cores ANSI aparecem em terminal; desligam automaticamente em pipes, com
> `--plain`, `NO_COLOR` ou `theme = none`.

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
