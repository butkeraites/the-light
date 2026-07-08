# Decisões de Arquitetura (ADRs curtos)

> Um registro por decisão relevante. Formato: contexto → decisão → consequência.
> Datas em ISO (YYYY-MM-DD).

## ADR-0001 — Toolchain Rust via rustup · 2026-06-15
**Contexto:** a máquina de desenvolvimento não tinha Rust instalado; a Fase 0 exige
`cargo build/clippy/test`.
**Decisão:** instalar via `rustup` (toolchain `stable`, perfil default com clippy e
rustfmt). Fixado em `rust-toolchain.toml` (channel `stable`).
**Consequência:** ambiente reprodutível; `rust-version` mínima declarada como 1.80 no
workspace. **Atualização (2026-06-16, Fase 7):** a MSRV subiu para **1.85** — o
conector HTTP (`reqwest 0.13`, adicionado na Fase 5) exige rustc 1.85. O job de
MSRV do CI valida 1.85.

## ADR-0002 — Workspace com 3 crates + xtask · 2026-06-15
**Contexto:** o plano (§1) pede separar lógica testável de CLI e TUI.
**Decisão:** `crates/the-light-core` (lógica pura), `crates/the-light-cli` (binário `light`),
`crates/the-light-tui` (Fase 3) e `xtask` (importadores one-off). `resolver = "2"`.
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
sobrescrevível por `LIGHT_SECRETS` (testes). O provedor ativo (não-secreto) fica
no `config.toml`. Integração com keychain do SO fica como evolução futura —
não é testável de forma determinística no ambiente atual e tocaria o keychain real.
**Consequência:** chaves nunca no git, nunca ecoadas; testes determinísticos via
`LIGHT_SECRETS`. `KeyStore::list_providers` expõe só nomes, nunca valores.

## ADR-0008 — Anti-alucinação: texto citado vem do banco, não do LLM · 2026-06-16
**Contexto:** risco de alucinação em temas teológicos (SPEC §9).
**Decisão:** o `StudyResult` separa **texto citado** (extraído do banco local,
exato) de **interpretação** (saída do LLM). O prompt recebe a passagem e os
xrefs locais (RAG leve) e instrui a citar versículos e separar texto de
interpretação, sinalizando divergências entre tradições.
**Consequência:** a citação é sempre fiel; a lente é explícita; testes usam
`MockLlmProvider` (sem rede), provedores reais só em teste manual documentado.

## ADR-0009 — HTTP de IA via reqwest (rustls), sem SDK · 2026-06-16
**Contexto:** o Rust não tem SDK oficial da Anthropic; os provedores (Anthropic/
OpenAI/Ollama) precisam de chamadas HTTP. A skill `claude-api` recomenda HTTP
direto para linguagens sem SDK.
**Decisão:** `reqwest` *blocking* com `default-tls` (resolve para `rustls` +
`rustls-platform-verifier` — sem OpenSSL/cmake, multiplataforma). Anthropic usa
`claude-opus-4-8` com `thinking: adaptive`; extrai só os blocos `type=="text"`.
Corpos de requisição e parsing são funções puras testadas; só `complete()` faz
I/O. Comparação de lentes via `study --lens a,b` (lista separada por vírgula) em
vez de um subcomando `compare` — mais simples e sem ambiguidade no clap.
**Consequência:** testes nunca tocam a rede (usam `MockLlmProvider`); provedores
reais exercitados só manualmente. `study`/`ask` degradam com erro amigável sem
chave/provedor ou offline.

## ADR-0010 — Conectores de versões protegidas (opt-in, ao vivo) · 2026-06-16
**Contexto:** versões sob copyright (ARA/NVI/ESV/…) não podem ser embarcadas; o
SPEC pede acesso só via conector com a credencial do usuário (§3, §5.2).
**Decisão:** dois conectores que implementam `BibleSource` no core —
`ApiBibleSource` (API.Bible, `data.content`, header `api-key`, ids USFM) e
`EsvApiSource` (ESV API, `passages[0]`, `Authorization: Token`, mantém a
atribuição "(ESV)"). Ambos `is_embeddable()=false`, `search→Unsupported`, e
devolvem `Passage` efêmera (um bloco). HTTP via `source::http` com verificação
de status **antes** do JSON (mesma lição do Marco 5). Mapeamento slug→fonte em
`config.connectors` (`config connector add/list/remove`); a chave fica no
cofre por tipo (`apibible`/`esv`). Resolução `crate::sources::resolve` (CLI):
versão local primeiro, senão conector; sem chave → erro claro **sem** rede.
`read`/`study`/`ask` passam a resolver versões locais **e** protegidas.
**Consequência:** fronteira legal isolada no tipo; nada protegido é embarcado ou
cacheado em massa; testes cobrem builders/parsing e o caminho sem-chave, nunca a
rede. Comparação `--bible-id` exigido para API.Bible; ESV não precisa.

## ADR-0011 — Distribuição do v1.0.0 sem crates.io · 2026-06-16
**Contexto:** o plano pede `cargo install`, Homebrew e binários. Publicar no
crates.io exige publicar também `the-light-core`/`the-light-tui` como pacotes públicos
(contratos de API a manter) e lida mal com o fluxo de banco/dados; o nome do
binário (`light`) difere do crate (`the-light-cli`).
**Decisão:** no v1.0.0 a distribuição é por **binários pré-compilados** (GitHub
Releases, multiplataforma + `.sha256`), **Homebrew** (tap) e **`cargo install
--git`/`--path`**. Todas as crates do workspace são `publish = false` (evita
publicação acidental). O `cargo install --git`/`--path` funciona mesmo com
`publish = false` (só bloqueia `cargo publish`). Publicar no crates.io fica como
evolução futura (os metadados de pacote já estão prontos no `the-light-cli`).
**Consequência:** README documenta os caminhos que funcionam de fato; nenhum
`cargo install the-light-cli` enganoso; sem etapa de `cargo publish` no release.

## ADR-0012 — Embarcar BSB (en) + Bíblia Livre (pt) · 2026-07-08
**Contexto:** a Rodada 3 do app pede amplitude (>2 versões). `DATA_SOURCES.md` já
pré-aprovava **BSB** (Berean Standard Bible, en) e **Bíblia Livre / BLIVRE** (pt)
como as substitutas EN/PT recomendadas; ambas caem nos parsers existentes
(`Scrollmapper` / `ThiagobodrukArray`), 66 livros, 31.102 versículos, sem código
novo. Mudança autorizada pelo mantenedor.
**Decisão:** registrar duas `TranslationSpec` LIVRES em `xtask/src/import.rs`:
`bsb` (`Berean Standard Bible`, `Lang::En`, `license="public-domain"` — CC0 desde
2023-04-30, tratada como a KJV, shape scrollmapper `master`) e `blivre`
(`Bíblia Livre`, `Lang::Pt`, `license="cc-by"`, damarals release **v1.0.0**
imutável). O `cc-by` é **hardcoded**: o mirror rotula "domínio público"
ERRADAMENTE — os termos vinculantes são **CC BY 3.0 Brasil**; `License::from`
mapeia `cc-by`→`Cc("cc-by")` e `is_embeddable()=true` (sem `nc`/`nd`). A
**atribuição obrigatória** da BLIVRE fica **app-side, verbatim** (molde já em
produção para OpenBible/STEP CC-BY) — o core **não** ganha coluna
`translations.attribution` (zero migração de schema). Import verificado numa DB
temporária: 31.102 versículos cada, `Gênesis 1:1` e `João 3:16` conferidos
verbatim em ambas. `fmt`/`clippy -D warnings`/`test` verdes.
**Consequência:** quatro versões locais (`kjv`/`alm1911`/`bsb`/`blivre`); tudo a
jusante é agnóstico à tradução (subset, seletor, paralelo, Compare). O app re-pina
o `the-light` neste commit, regenera os assets sqlite e passa a exibir o crédito
CC-BY da BLIVRE verbatim. Nenhuma versão protegida é embarcada (o allowlist do
`SPECS` segue a regra de ouro do `DATA_SOURCES.md`).

## ADR-0004 — Licença do código `MIT OR Apache-2.0` · 2026-06-15
**Contexto:** o SPEC sugere MIT ou Apache-2.0; convenção do ecossistema Rust é dupla.
**Decisão:** `MIT OR Apache-2.0` para o código. Dados bíblicos seguem suas próprias
licenças (ver `DATA_SOURCES.md`); só versões de domínio público são embarcadas.
**Consequência:** máxima compatibilidade de reuso; fronteira legal de dados isolada.
