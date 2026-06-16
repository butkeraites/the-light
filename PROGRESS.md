# Log de Execução

> Uma linha por tarefa concluída e verde: ID, data, resumo, hash do commit.
> Ver `IMPLEMENTATION_PLAN.md` §6 para a definição das tarefas.

## Fase 7 — Polimento & lançamento aberto

| Tarefa | Data | Resumo | Commit |
|---|---|---|---|
| T7.1 | 2026-06-16 | Empacotamento: versão do workspace → `1.0.0` (pins internos sincronizados); metadados crates.io no `biblia-cli` (keywords/categories/readme); CI (`.github/workflows/ci.yml`: fmt+clippy, testes Linux/macOS/Windows, build MSRV 1.80); release (`release.yml`: binários x86_64/aarch64 + .sha256 em tags `v*`); fórmula Homebrew (`packaging/homebrew/biblia.rb`) | _pendente_ |
| T7.2 | 2026-06-16 | Documentação: README com instalação + privacidade/telemetria-zero + licença; `--help` de qualidade (long_about + exemplos); guia de prompts editáveis (`docs/PROMPTS.md`); DATA_SOURCES §4 (conectores) | _pendente_ |
| T7.3 | 2026-06-16 | Hardening: `LICENSE-MIT` + `LICENSE-APACHE`; telemetria zero documentada; deps internas com versão (publicável); fix do `--version` (colisão clap `propagate_version` × flag de versão da Bíblia); testes de metadados (`meta_cmd.rs`: --version/--help/subcomandos) | _pendente_ |

**Marco 7** (2026-06-16): release pública. Tag `v1.0.0`. Suíte: 243 testes,
`clippy -D warnings` e `fmt` verdes.

## Fase 6 — Conectores de versões protegidas

| Tarefa | Data | Resumo | Commit |
|---|---|---|---|
| T6.1 | 2026-06-16 | `ApiBibleSource` (API.Bible, opt-in): `data.content` via `passages/{USFM id}`, header `api-key`, texto puro; `is_embeddable=false`, `search→Unsupported`; `source::http` (status-first). Endpoints verificados em doc | d91423a |
| T6.2 | 2026-06-16 | `EsvApiSource` (ESV API, opt-in): `passages[0]` via `?q=`, `Authorization: Token`, mantém atribuição "(ESV)". `config connector add/list/remove`, cofre por tipo, `set-key apibible/esv`; resolver `sources::resolve` (local→conector); `read`/`study`/`ask` leem versões protegidas; aviso de uso/citação | d91423a |

**Marco 6** (2026-06-16): versões protegidas sob credencial do usuário (BYOK,
nunca embarcadas/cacheadas). Tag `v0.7.0`. Suíte: ~236 testes, `clippy -D warnings` e `fmt` verdes.

**Revisão adversarial pós-Marco 6** (2026-06-16): 23 achados → 15 confirmados (5 positivos) →
corrigidos: **(crítico)** `study --save` não persiste mais texto de versão protegida
(efêmero — SPEC §5.2); `study`/`ask` avisam ao usar conector (texto vai ao provedor de IA);
`read` usa saída 1 (não 2) em versão desconhecida/falha parcial (ADR-0006); `EsvApiSource`
devolve `Passage` vazia (não erro) em passagem sem texto, igual ao API.Bible; isolamento de
`BIBLIA_SECRETS` nos testes de read/search/tui; testes de study/ask com versão protegida
sem chave (sem rede). Suíte: 238 testes. (commit 9c08d68)

## Fase 5 — Camada de IA (BYOK)

| Tarefa | Data | Resumo | Commit |
|---|---|---|---|
| T5.1 | 2026-06-16 | Abstração `LlmProvider` + tipos (Denomination/StudyDepth) + `MockLlmProvider`; `KeyStore` (secrets.toml 0600, fora do git, `BIBLIA_SECRETS`); `config provider` + `config set-key/remove-key/keys`; ADR-0007/0008; 6 core + 3 integração | bf6bab8 |
| T5.2 | 2026-06-16 | `ai/prompts.rs`: system prompt por lente (6 tradições) + profundidade, override por arquivo local (`BIBLIA_PROMPTS`/`prompts/<slug>.md`); `ask_system_prompt`; 6 testes | 4b687e9 |
| T5.3 | 2026-06-16 | `ai/study.rs` (orquestração RAG leve: passagem + xrefs no contexto, separa texto citado da interpretação, `to_markdown`) + CLI `study "Ef 2.8-9" --lens <l> [--depth] [--save]`; estimativa tokens/custo; erros amigáveis sem chave/provedor | 4b687e9, 9df6c84 |
| T5.4 | 2026-06-16 | Comparar lentes via `study --lens a,b` (em vez de subcomando `compare`); `ask "..." --ref "Rm 3"` (RAG); 8 testes de integração (provedor `mock`) | 9df6c84 |
| T5.5 | 2026-06-16 | Provedores `anthropic` (HTTP direto, `claude-opus-4-8`, pensamento adaptativo), `openai`, `ollama` (local) + fábrica `build_provider` + `estimate_cost_usd`; corpos/parsing como funções puras testadas sem rede (9 testes) | 9df6c84 |

**Marco 5** (2026-06-16): estudo assistido por IA (BYOK), opt-in, anti-alucinação.
Tag `v0.6.0`. Suíte: 218 testes, `clippy -D warnings` e `fmt` verdes.

**Revisão adversarial pós-Marco 5** (2026-06-16): 14 achados → 10 confirmados →
corrigidos: **(crítico)** providers checam o status HTTP antes de exigir JSON
(erro de API legível, não "parse error"); xrefs do RAG agregam **toda a passagem**
(não só o v.1); `study` multi-lente sinaliza falha parcial (saída ≠ 0 + aviso);
`Secrets` com `Debug` que redige as chaves; `ask` reusa `numbered_passage` (sem
braço morto) + marca xrefs vazios + avisa `--db` sem `--ref`; mensagens de
"chave desconhecida" do `config` incluem `provider`. Suíte: 220 testes. (commit 2ba32bd)

## Fase 4 — Planos de leitura

| Tarefa | Data | Resumo | Commit |
|---|---|---|---|
| T4.1 | 2026-06-16 | Engine de planos (`userdata/plans.rs`): anual/NT/evangelhos a partir dos capítulos canônicos (1189), progresso por data (injetável) + persistência; `plan list/start/today/status/mark/reset`; 6 testes core + integração | 813d0cf |
| T4.2 | 2026-06-16 | Export `.ics` (`plan ics`): VEVENT por dia (all-day), escape RFC 5545; calendário válido verificável | 813d0cf |

**Marco 4** (2026-06-16): planos de leitura com acompanhamento. Tag `v0.5.0`.
Suíte: 187 testes + 1 doctest, `clippy -D warnings` e `fmt` verdes.

**Revisão adversarial pós-Marco 4** (2026-06-16): 9 achados → 6 confirmados → corrigidos:
`plan start` exige `--force` p/ sobrescrever; `status` faz clamp de `completed`;
`.ics` com line folding RFC 5545. (Achado de "escapar aspas" rejeitado: o RFC não
escapa aspas em valores TEXT.) (commit 6037eb4)

**Revisão adversarial pós-Marco 3** (2026-06-16): 24 achados → 10 confirmados (3 positivos) →
corrigidos: navegação atômica da TUI (estado consistente em falha), guarda de janela
minúscula, ciclo de tema visível a partir de "auto", clamp de `book_idx`. (commit 15261d1)

## Fase 3 — TUI (ratatui)

| Tarefa | Data | Resumo | Commit |
|---|---|---|---|
| T3.1 | 2026-06-16 | Shell da TUI (`biblia-tui`): guarda RAII do terminal + hook de panic, layout (livros + viewport), navegação por teclado (livros/capítulos/scroll), `biblia tui`; 7 testes (estado + snapshot TestBackend) + 2 integração | 07d29f7 |
| T3.2 | 2026-06-16 | Viewport rolável + troca de versão (`v`, mantém passagem) + ir-para-referência (`g`, prompt com erro); 9 testes TUI | a0d0719 |
| T3.3 | 2026-06-16 | Cursor de versículo + painel lateral (marcações/nota/xref sincronizados) + navegação de xref na TUI (`x`, Enter salta); 9 testes TUI | 50fa430 |
| T3.4 | 2026-06-16 | Busca interativa (`/`) + tema (`t`) persistido; 14 testes TUI | 152a5b2 |

**Marco 3** (2026-06-16): TUI completa de leitura/estudo. Tag `v0.4.0`.
Suíte: 171 testes + 1 doctest, `clippy -D warnings` e `fmt` verdes.

## Fase 2 — Estudo pessoal (offline)

| Tarefa | Data | Resumo | Commit |
|---|---|---|---|
| T2.1 | 2026-06-16 | Highlights (`userdata/highlights.rs`): `highlight add/list/remove`, `highlights.json` legível e atômico, aparece no rodapé da leitura; `BIBLIA_DATA_DIR`; util atômico compartilhado; 5 testes core + 4 integração | c2b10fa |
| T2.2 | 2026-06-16 | Notas (`userdata/notes.rs`): `note add/edit/show/list/remove`, uma `.md` por nota, `$EDITOR`, render Markdown (`md.rs` via pulldown-cmark), rodapé na leitura; 5 testes core + 3 unit md + 5 integração | 357e753 |
| T2.3 | 2026-06-16 | Referências cruzadas (`xref.rs` + `xtask import-xref`): OpenBible/TSK (CC-BY), 344.799 xrefs, OSIS via `book_number`, `biblia xref` com votos/limiar e texto; 4 core + 5 xtask + 4 integração | 6e8fd8c |
| T2.4 | 2026-06-16 | Export (`export.rs`): notes/study em md/pdf (pandoc); 5 testes | 41041a7 |

**Marco 2** (2026-06-16): estudo pessoal completo offline. Tag `v0.3.0`.
Suíte: 152 testes + 1 doctest, `clippy -D warnings` e `fmt` verdes.

## Fase 1 — Leitura & busca

| Tarefa | Data | Resumo | Commit |
|---|---|---|---|
| T1.1 | 2026-06-16 | Leitura paralela: colunas lado a lado (com quebra de linha) e blocos intercalados alinhados por versículo; módulo `render` (9 testes) | 05e7686 |
| T1.2 | 2026-06-16 | Busca FTS5 (`search.rs` + subcomando): acento-insensível, BM25, AND de palavras, filtro de livro, destaque; 8 testes core + 6 integração | 70e31f5 |
| T1.3 | 2026-06-16 | Config `config.toml` (XDG, env BIBLIA_CONFIG): `config set/get/list`; read/search usam versões padrão; 6 testes core + 6 integração | cc6862b |
| T1.4 | 2026-06-16 | Tema/cores ANSI: número, referência e destaque coloridos; --plain, NO_COLOR, auto-TTY; 3 testes theme + 1 integração | c8df604 |

**Marco 1** (2026-06-16): CLI de leitura/busca utilizável no dia a dia. Tag `v0.2.0`.

**Revisão adversarial pós-Marco 1** (2026-06-16): 18 achados → 13 confirmados →
corrigidos os relevantes: alinhamento de colunas com versificação divergente
(separador pendurado); gravação atômica do `config.toml`; exit code do `search`
em banco vazio (→1, igual ao read); clamp do `limit` da busca; `read` cai para
`kjv` quando nada é pedido/configurado. Suíte: 112 testes + 1 doctest verdes.

## Fase 0 — Fundação

| Tarefa | Data | Resumo | Commit |
|---|---|---|---|
| Setup | 2026-06-15 | Docs de governança (DATA_SOURCES/DECISIONS/PROGRESS) + git init | 6da176d |
| T0.1 | 2026-06-15 | Scaffolding do workspace (3 crates + xtask), compila verde | 82e18ab |
| T0.2 | 2026-06-15 | Modelo de domínio + parser de referências PT/EN (66 livros, 39 testes) | 872138c |
| T0.3 | 2026-06-15 | Store SQLite + migração v1 + FTS5 validado (busca sem acento), 6 testes | c680a3e |
| T0.4 | 2026-06-15 | Importador (xtask): KJV 31.102 + Almeida 1911 31.101 versículos; idempotente; 5 testes | 48fac31 |
| T0.5 | 2026-06-15 | Camada de fontes (BibleSource/EmbeddedSource) + comando `read` (PT/EN, intervalo, capítulo); 6 testes de integração | 37ae833 |

**Marco 0 alcançado** (2026-06-15): ler um versículo offline em PT e EN. Tag `v0.1.0`.
Suíte: 62 testes + 1 doctest, `clippy -D warnings` e `fmt` verdes.

**Revisão adversarial pós-Marco 0** (2026-06-15): 27 achados → 9 confirmados → corrigidos
os relevantes: código de saída do `read` em falha parcial de versão; dedup de versões;
`License::is_embeddable` agora rejeita CC NC/ND; render de versículo via `start()`;
entrada UTF-8 decomposta (NFD); alias duplicado. Suíte: 68 testes + 1 doctest verdes.
