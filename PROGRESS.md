# Log de Execução

> Uma linha por tarefa concluída e verde: ID, data, resumo, hash do commit.
> Ver `IMPLEMENTATION_PLAN.md` §6 para a definição das tarefas.

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
