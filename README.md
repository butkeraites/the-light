# Bíblia CLI

> Leitor de Bíblia hackeável para terminal, com estudo exegético assistido por IA
> (lente denominacional configurável), dados locais e modelo *bring-your-own-key*.

Status: **Fase 4 — Planos de leitura concluída** (Marco 4, `v0.5.0`): planos anual/NT/
evangelhos com progresso e export `.ics`, sobre a TUI (Fase 3), estudo pessoal (Fase 2)
e leitura/busca (Fase 1). Veja [`SPEC.md`](SPEC.md) para a visão e
[`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) para o roadmap.

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

# 7. Planos de leitura (anual/NT/evangelhos) com progresso e calendário:
cargo run -p biblia-cli -- plan start annual --year 2026
cargo run -p biblia-cli -- plan today
cargo run -p biblia-cli -- plan ics --output plano.ics   # importável no calendário

# 8. Estudo com IA (opt-in, BYOK — sua própria chave, fora do git):
cargo run -p biblia-cli -- config set provider anthropic
cargo run -p biblia-cli -- config set-key anthropic sk-ant-...   # grava em secrets.toml (0600)
cargo run -p biblia-cli -- study "Ef 2.8-9" --lens presbiteriana --depth exegetico --db data/biblia.sqlite
cargo run -p biblia-cli -- study "Ef 2.8-9" --lens batista,luterana --db data/biblia.sqlite  # compara lentes
cargo run -p biblia-cli -- ask "Como Paulo define a graça?" --ref "Rm 3" --db data/biblia.sqlite
cargo run -p biblia-cli -- study "Jo 1.1" --lens batista --provider mock   # demonstração sem rede/chave

# 9. Versões protegidas (opt-in, lidas ao vivo com a SUA chave — nunca embarcadas):
cargo run -p biblia-cli -- config connector add ara --kind apibible --bible-id <bibleId> --name "Almeida Revista e Atualizada" --abbrev ARA --lang pt
cargo run -p biblia-cli -- config set-key apibible <chave-api.bible>
cargo run -p biblia-cli -- read "Jo 3.16" --version kjv,ara --db data/biblia.sqlite   # livre + protegida lado a lado
cargo run -p biblia-cli -- config connector add esv --kind esv --name "English Standard Version" --abbrev ESV --lang en
cargo run -p biblia-cli -- config set-key esv <chave-esv>
cargo run -p biblia-cli -- read "John 3:16" --version esv --db data/biblia.sqlite
```

> Versões protegidas (ARA/NVI/ESV/…) **nunca são embarcadas nem cacheadas em
> massa**: são buscadas ao vivo via conector (API.Bible/ESV) sob a credencial do
> próprio usuário, que aceita os termos da API. Sem chave → indisponível com
> mensagem clara (sem chamada de rede). Ver `DATA_SOURCES.md` §4.

> Dados do usuário (notas `.md`, `highlights.json`, estudos `.md`) vivem em
> arquivos abertos e versionáveis sob o diretório de dados do SO (ou `BIBLIA_DATA_DIR`).

> A IA é **opt-in e BYOK**: provedores `anthropic`/`openai`/`ollama` (local). A
> chave fica num `secrets.toml` (`0600`, fora do git, ou `BIBLIA_SECRETS`), nunca
> é ecoada. O estudo separa o **texto citado** (do acervo local, exato) da
> **interpretação** (do modelo); prompts de lente são editáveis em `prompts/<lente>.md`.

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
