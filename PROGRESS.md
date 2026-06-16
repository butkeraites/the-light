# Log de Execução

> Uma linha por tarefa concluída e verde: ID, data, resumo, hash do commit.
> Ver `IMPLEMENTATION_PLAN.md` §6 para a definição das tarefas.

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
