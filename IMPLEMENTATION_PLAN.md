# Bíblia CLI — Plano de Implementação (para execução pelo Claude Code)

> **Como usar este documento:** este é um plano executável. O agente deve ler
> primeiro o `SPEC.md` (visão e arquitetura) e depois executar as tarefas abaixo
> **em ordem**, fase a fase. Cada tarefa tem: objetivo, arquivos, notas de
> implementação, critério de aceite e comando de verificação. Não pule a
> verificação. Faça commit ao final de cada tarefa concluída e verde.

Stack: **Rust** (edition 2021). Conteúdo inicial: **bilíngue PT + EN**.
Documento v0.1 · 2026-06-15

---

## 0. Regras de trabalho para o agente

1. **Leia `SPEC.md` antes de começar.** Ele é a fonte de verdade de design; este arquivo é o "como".
2. **Trabalhe fase a fase, tarefa a tarefa.** Não comece uma fase sem a anterior verde.
3. **TDD onde fizer sentido.** Escreva testes junto com cada módulo de lógica (parser, modelo, busca). UI pode ser testada manualmente + snapshot.
4. **Padrão de qualidade por tarefa:** `cargo fmt`, `cargo clippy -- -D warnings` e `cargo test` precisam passar antes do commit.
5. **Commits pequenos e descritivos** (Conventional Commits: `feat:`, `fix:`, `test:`, `chore:`, `docs:`). Um commit por tarefa concluída, prefixado com o ID (ex.: `feat(T1.2): full-text search via FTS5`).
6. **Licenciamento é regra rígida.** Só embarcar versões de domínio público/licença livre. Versões protegidas (ARA, NVI, ESV, NIV…) **nunca** entram no binário nem em cache em massa — apenas via conector em tempo real com a key do usuário. Em caso de dúvida sobre a licença de um dataset, **pare e registre em `DATA_SOURCES.md` em vez de embarcar**.
7. **Sem segredos no repo.** Keys de API ficam fora do git; usar `.gitignore` e armazenamento do SO.
8. **Documente decisões** num `DECISIONS.md` (ADR curto) sempre que escolher entre alternativas relevantes (ex.: crate de TUI, formato de dados do usuário).
9. **Atualize `PROGRESS.md`** ao concluir cada tarefa: ID, data, resumo, hash do commit.

---

## 1. Estrutura-alvo do repositório

```
biblia/
├── Cargo.toml                  # workspace
├── README.md
├── SPEC.md                     # já existe
├── IMPLEMENTATION_PLAN.md      # este arquivo
├── DATA_SOURCES.md             # proveniência + licença de cada dataset
├── DECISIONS.md                # ADRs curtos
├── PROGRESS.md                 # log de execução
├── .gitignore
├── crates/
│   ├── biblia-core/            # lógica pura, sem I/O de terminal
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── model.rs        # Reference, Passage, Verse, Translation...
│   │   │   ├── reference.rs    # parser de referências PT/EN
│   │   │   ├── source/         # camada de fontes (trait + impls)
│   │   │   │   ├── mod.rs      # trait BibleSource
│   │   │   │   ├── embedded.rs # SQLite local
│   │   │   │   ├── api_bible.rs# conector API.Bible (opt-in)
│   │   │   │   └── esv.rs      # conector ESV (opt-in)
│   │   │   ├── search.rs       # busca FTS5
│   │   │   ├── store.rs        # abertura/migração do SQLite
│   │   │   ├── userdata/       # notas, highlights, planos
│   │   │   │   ├── mod.rs
│   │   │   │   ├── notes.rs
│   │   │   │   ├── highlights.rs
│   │   │   │   └── plans.rs
│   │   │   ├── xref.rs         # referências cruzadas (TSK)
│   │   │   ├── ai/             # camada LLM (opcional, BYOK)
│   │   │   │   ├── mod.rs      # trait LlmProvider
│   │   │   │   ├── prompts.rs  # system prompts por lente denominacional
│   │   │   │   ├── openai.rs
│   │   │   │   ├── anthropic.rs
│   │   │   │   └── ollama.rs
│   │   │   └── config.rs       # config.toml + paths XDG
│   │   └── tests/
│   ├── biblia-cli/             # binário: clap + subcomandos
│   │   └── src/main.rs
│   └── biblia-tui/             # binário/lib: ratatui
│       └── src/...
├── data/
│   ├── importer/               # scripts/bin de import (one-off)
│   └── seed/                   # datasets brutos (livres) versionados ou baixados
└── xtask/                      # tarefas de build/import (cargo xtask)
```

> Workspace com 3 crates separa **lógica** (testável) de **CLI** e **TUI**.

---

## 2. Dependências (Cargo)

Fixar majors; deixar o `cargo` resolver minors. Confirmar versões mais recentes no momento da execução.

```toml
# biblia-core
clap        = { version = "4", features = ["derive"] }   # só em cli; core não depende
rusqlite    = { version = "0.31", features = ["bundled"] } # SQLite embutido + FTS5
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
toml        = "0.8"
directories = "5"        # paths XDG (config/data)
anyhow      = "1"        # erros em binários
thiserror   = "1"        # erros tipados na lib
pulldown-cmark = "0.11"  # render markdown de notas
regex       = "1"        # parser de referências

# rede (conectores API + LLM)
reqwest     = { version = "0.12", features = ["json", "rustls-tls"] }
tokio       = { version = "1", features = ["rt-multi-thread", "macros"] }

# tui (biblia-tui)
ratatui     = "0.28"
crossterm   = "0.28"

# import / progresso
indicatif   = "0.17"

# dev
[dev-dependencies]
insta       = "1"        # snapshot tests
tempfile    = "3"
```

> Se `rusqlite` com `bundled` compilar FTS5 por padrão; caso contrário, habilitar a feature de FTS5. Validar em T0.3.

---

## 3. Modelo de dados (DDL de referência)

Migrações em `crates/biblia-core/migrations/` aplicadas na abertura do banco
(checar `PRAGMA user_version`).

```sql
-- v1: texto bíblico (read-only)
CREATE TABLE translations (
  id         TEXT PRIMARY KEY,         -- 'kjv', 'bsb', 'almeida-1911'
  abbrev     TEXT NOT NULL,
  name       TEXT NOT NULL,
  language   TEXT NOT NULL,            -- 'en', 'pt'
  license    TEXT NOT NULL,            -- 'public-domain', 'cc-by', ...
  embeddable INTEGER NOT NULL          -- 1 = pode redistribuir
);

CREATE TABLE books (
  id            INTEGER PRIMARY KEY,
  translation_id TEXT NOT NULL REFERENCES translations(id),
  number        INTEGER NOT NULL,      -- ordem canônica 1..66(+)
  name          TEXT NOT NULL,
  abbrev        TEXT NOT NULL,
  testament     TEXT NOT NULL          -- 'OT' | 'NT'
);

CREATE TABLE verses (
  id            INTEGER PRIMARY KEY,
  translation_id TEXT NOT NULL REFERENCES translations(id),
  book_number   INTEGER NOT NULL,
  chapter       INTEGER NOT NULL,
  verse         INTEGER NOT NULL,
  text          TEXT NOT NULL,
  UNIQUE(translation_id, book_number, chapter, verse)
);

CREATE VIRTUAL TABLE verses_fts USING fts5(
  text, translation_id UNINDEXED, verse_id UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 2'   -- busca PT sem acento
);

CREATE TABLE cross_references (    -- Treasury of Scripture Knowledge (domínio público)
  from_book INTEGER, from_chapter INTEGER, from_verse INTEGER,
  to_book   INTEGER, to_chapter   INTEGER, to_verse_start INTEGER, to_verse_end INTEGER,
  votes     INTEGER
);
```

Dados do usuário **não** ficam no SQLite do texto — ficam em arquivos (ver §4 do SPEC):
`config.toml`, `notes/*.md`, `highlights.json`, `reading-plans/*.json`, `studies/*.md`.
Motivo: versionável em git, portável, inspecionável.

---

## 4. Contratos de código (assinaturas-chave)

```rust
// model.rs
pub struct Reference {
    pub book: u8,            // número canônico
    pub chapter: u16,
    pub verses: VerseRange,  // single | range | whole-chapter
}
pub struct Verse { pub reference: Reference, pub text: String, pub translation: TranslationId }
pub struct Passage { pub reference: Reference, pub verses: Vec<Verse> }
pub struct Translation { pub id: TranslationId, pub abbrev: String, pub name: String,
                         pub language: Lang, pub license: License, pub embeddable: bool }

// source/mod.rs
pub trait BibleSource {
    fn translations(&self) -> Vec<Translation>;
    fn passage(&self, r: &Reference, t: &TranslationId) -> anyhow::Result<Passage>;
    fn search(&self, query: &str, t: &TranslationId, limit: usize) -> anyhow::Result<Vec<SearchHit>>;
    fn is_embeddable(&self) -> bool;
}

// ai/mod.rs
pub struct StudyRequest<'a> {
    pub passage: &'a Passage,
    pub lens: Denomination,        // Baptist, Presbyterian, Lutheran, Pentecostal, Catholic, Orthodox, Compare
    pub language: Lang,
    pub depth: StudyDepth,         // Overview | Exegetical | WordStudy
}
pub trait LlmProvider {
    fn study(&self, req: &StudyRequest) -> anyhow::Result<StudyResult>;
    fn ask(&self, question: &str, context: &[Verse]) -> anyhow::Result<String>;
}
```

Regra de saída da IA: `StudyResult` deve conter campos separados para **texto
citado** (versículos referenciados) e **interpretação**, e marcar a lente usada.
Os prompts (em `ai/prompts.rs`) instruem o modelo a citar referências e sinalizar
divergências entre tradições.

---

## 5. Fontes de dados (a confirmar antes de embarcar)

Registrar tudo em `DATA_SOURCES.md` com URL, licença e data de verificação.

**EN (livres, embarcar):** KJV, ASV (1901), BSB/Berean (domínio público desde 2023), WEB, YLT.
Boas origens: `ebible.org`, `scrollmapper/bible_databases`, `wldeh/bible-api`.

**PT (livres, embarcar — verificar versão a versão):** Almeida 1911 / domínio público; dataset `thiagobodruk/biblia` (XML/SQL/JSON).

**Referências cruzadas:** Treasury of Scripture Knowledge (TSK), domínio público (ex.: `OpenBible.info` cross-references, CC-BY — checar atribuição).

**NÃO embarcar (conector opt-in com key do usuário):** ARA, ARC, NTLH, NVI (SBB não autoriza); ESV, NIV, NASB, CSB, NLT (uso comercial restrito; citação ≤500 versículos / ≤25%). Acesso via API.Bible / ESV API.

> Tarefa explícita: o importador deve gravar `license` e `embeddable` corretos por tradução. Se a licença não puder ser confirmada, marcar `embeddable=0` e não incluir o texto no binário.

---

## 6. Fases e tarefas

### FASE 0 — Fundação

**T0.1 — Scaffolding do workspace**
Criar workspace com as 3 crates, `.gitignore`, `README.md` mínimo, CI local (`cargo fmt/clippy/test`).
*Aceite:* `cargo build` compila workspace vazio.
*Verificar:* `cargo build && cargo clippy -- -D warnings`.

**T0.2 — Modelo + parser de referências (`model.rs`, `reference.rs`)**
Implementar tipos do §4 e parser que aceite PT/EN: `Jo 3.16`, `John 3:16`, `Gn 1.1-3`, `Sl 23`, `1Co 13.4-7`, listas. Tabela de aliases de livros (66 livros) em PT e EN.
*Aceite:* testes cobrindo ≥20 formatos válidos e ≥8 inválidos.
*Verificar:* `cargo test -p biblia-core reference`.

**T0.3 — Store SQLite + migrações (`store.rs`)**
Abrir/criar DB em path XDG, aplicar migração v1 (§3), validar FTS5 disponível.
*Aceite:* teste cria DB temporário, roda migração, confirma tabelas e FTS5.
*Verificar:* `cargo test -p biblia-core store`.

**T0.4 — Importador de uma versão livre PT e uma EN (`data/importer`, `xtask`)**
Baixar/parsear datasets livres (ex.: BSB em EN, Almeida domínio público em PT), popular `translations/books/verses/verses_fts` com `license`/`embeddable` corretos. Idempotente.
*Aceite:* após import, `SELECT count(*) FROM verses` > 30.000 por versão; `Jo 3.16` retorna texto correto nas duas línguas.
*Verificar:* `cargo run -p xtask -- import --version bsb,almeida-pd` e teste de smoke.

**T0.5 — Comando `read` (CLI mínima)**
`biblia read "John 3:16" --version kjv` imprime o versículo. Erros amigáveis.
*Aceite:* saída correta para passagem e intervalo; código de saída ≠0 em referência inválida.
*Verificar:* teste de integração via `assert_cmd` ou execução manual documentada.

**Marco 0:** ler um versículo offline em PT e EN. Tag `v0.1.0`.

---

### FASE 1 — Leitura & busca

**T1.1 — Múltiplas versões e leitura paralela**
`--version a,b` renderiza colunas/blocos lado a lado.
*Aceite:* dois textos alinhados por versículo.

**T1.2 — Busca full-text (`search.rs`)**
`biblia search "graça" --version almeida-pd [--book Romanos] [--limit N]` via FTS5, com destaque do termo e ranqueamento.
*Aceite:* busca com e sem acento retorna resultados; filtro por livro funciona; teste com termos conhecidos.
*Verificar:* `cargo test -p biblia-core search`.

**T1.3 — Configuração (`config.toml`)**
`biblia config set|get|list`: versões padrão, idioma, tema, tamanho de fonte. Paths XDG.
*Aceite:* round-trip de config persistido; defaults sensatos.

**T1.4 — Tema/cores e formatação de saída**
Cores ANSI, destaque de número de versículo, modo `--plain` (sem cor, para pipes).
*Aceite:* saída legível com e sem TTY.

**Marco 1:** CLI de leitura/busca utilizável no dia a dia. Tag `v0.2.0`.

---

### FASE 2 — Estudo pessoal (offline)

**T2.1 — Highlights (`userdata/highlights.rs`)**
`biblia highlight "Jo 3.16" --color yellow --tag salvação`; listar/remover. Persistir em `highlights.json`. Mostrar marcações ao ler.
*Aceite:* highlight persiste e aparece na leitura; teste de round-trip.

**T2.2 — Notas (`userdata/notes.rs`)**
`biblia note add|edit|show|list` associadas a versículo/intervalo; uma `.md` por nota; abre `$EDITOR` quando sem texto inline.
*Aceite:* nota criada, listada e exibida na leitura; render markdown.

**T2.3 — Referências cruzadas (`xref.rs` + import TSK)**
Importar TSK; `biblia xref "Rm 3.23"` lista versículos relacionados; navegação encadeada.
*Aceite:* xrefs conhecidas retornam; import idempotente; licença registrada em `DATA_SOURCES.md`.

**T2.4 — Export**
`biblia export notes|study --format md|pdf` (PDF via pipeline simples ou `--format md` + ferramenta externa documentada).
*Aceite:* arquivo gerado com conteúdo correto.

**Marco 2:** estudo pessoal completo offline. Tag `v0.3.0`.

---

### FASE 3 — TUI (ratatui)

**T3.1 — Shell da TUI (`biblia-tui`)**
Loop de eventos `crossterm`, layout base (lista de livros/capítulos + viewport), navegação por teclado, sair com `q/Esc`.
*Aceite:* abre, navega, fecha sem corromper terminal (restaura modo).

**T3.2 — Leitura com viewport rolável + troca de versão**
Rolagem, ir-para-referência, alternar/adicionar versões em tela.
*Aceite:* rolagem fluida; troca de versão sem sair da passagem.

**T3.3 — Painéis de notas/highlights/xref**
Painel lateral mostrando notas/marcações do versículo atual; saltar por xref dentro da TUI.
*Aceite:* painel sincroniza com cursor.

**T3.4 — Busca interativa + fonte/tamanho/tema**
Barra de busca incremental; escolha de tema; ajuste de tamanho/espaçamento onde o terminal permitir; degradação graciosa.
*Aceite:* busca interativa filtra; preferências persistem em `config.toml`.

**Marco 3:** TUI completa de leitura/estudo. Tag `v0.4.0`.

---

### FASE 4 — Planos de leitura

**T4.1 — Engine de planos (`userdata/plans.rs`)**
Planos cronológico/anual/temático como JSON; `biblia plan start|today|status|reset`; progresso persistido.
*Aceite:* `plan today` mostra leitura do dia conforme data; progresso avança.

**T4.2 — Lembretes/calendário (opcional)**
Export `.ics` ou integração com calendário do SO para a leitura diária.
*Aceite:* arquivo `.ics` válido gerado (verificável).

**Marco 4:** planos de leitura com acompanhamento. Tag `v0.5.0`.

---

### FASE 5 — Camada de IA (BYOK)

**T5.1 — Abstração de provedor + gestão de keys (`ai/mod.rs`, `config.rs`)**
Trait `LlmProvider`; `biblia config set-key <provider> <key>` em armazenamento seguro do SO (keychain quando possível; senão arquivo com permissão restrita, fora do git). Selecionar provedor ativo.
*Aceite:* key gravada/lida sem aparecer em git; troca de provedor funciona.

**T5.2 — Prompts de lente denominacional (`ai/prompts.rs`)**
System prompts versionados/editáveis por denominação (Batista, Presbiteriana, Luterana, Pentecostal, Católica, Ortodoxa) + modo "comparar". Cada prompt exige citar versículos e separar texto de interpretação.
*Aceite:* prompts carregáveis; usuário pode sobrescrever via arquivo local.

**T5.3 — Estudo exegético**
`biblia study "Ef 2.8-9" --lens presbiteriana [--depth exegetical|wordstudy]`. Monta contexto (passagem + xrefs locais → RAG leve), chama provedor, salva em `studies/*.md`.
*Aceite:* gera estudo citando a passagem; falha amigável sem key; offline → mensagem clara.

**T5.4 — Comparar perspectivas + perguntas livres**
`biblia study compare "Tg 2.24" --lens batista,luterana`; `biblia ask "..." --ref "Rm 3"`.
*Aceite:* saída lado a lado; respostas ancoradas em referências fornecidas.

**T5.5 — Provedores: OpenAI, Anthropic, Ollama**
Implementações concretas + estimativa de tokens/custo antes de chamar.
*Aceite:* cada provedor responde em teste manual documentado; Ollama funciona offline-local.

**Marco 5:** estudo assistido por IA com lente teológica. Tag `v0.6.0`.

---

### FASE 6 — Conectores de versões protegidas

**T6.1 — `ApiBibleSource` (opt-in)**
Buscar ARA/NVI/etc. via API.Bible com key do usuário; nunca cachear em massa; respeitar limites.
*Aceite:* com key válida, passagem protegida é lida ao vivo; sem key, indisponível com mensagem clara.

**T6.2 — `EsvApiSource` (opt-in)**
Conector ESV análogo, respeitando limite de citação.
*Aceite:* leitura ao vivo com key; aviso de limites de uso.

**Marco 6:** versões protegidas acessíveis sob credencial do usuário. Tag `v0.7.0`.

---

### FASE 7 — Polimento & lançamento aberto

**T7.1 — Empacotamento/distribuição:** `cargo install`, fórmula Homebrew, binários em GitHub Releases (Linux/macOS/Windows) via CI.
**T7.2 — Documentação:** README completo, `--help` de qualidade, guia de prompts editáveis, `DATA_SOURCES.md` final.
**T7.3 — Hardening:** revisão de licenças, testes de borda, mensagens de erro, telemetria zero por padrão.
*Marco 7:* release pública `v1.0.0`.

---

## 7. Estratégia de testes

- **Unitários** (core): parser de referências, modelo, busca, planos, montagem de prompt. Alvo: alta cobertura na lógica pura.
- **Integração:** importador → DB → `read`/`search` ponta a ponta com DB temporário (`tempfile`).
- **CLI:** `assert_cmd` para códigos de saída e saída de texto.
- **Snapshot** (`insta`): saída formatada de `read`/`study` (mockando o provedor LLM).
- **LLM:** provedor `MockLlmProvider` para testes determinísticos; provedores reais só em testes manuais documentados (sem key em CI).
- **CI:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` em cada push.

---

## 8. Definição de pronto (Definition of Done) por tarefa

Uma tarefa só é "done" quando: código + testes escritos; `fmt`/`clippy`/`test`
verdes; critério de aceite satisfeito e verificado pelo comando indicado;
`PROGRESS.md` atualizado; commit feito com o ID da tarefa.

---

## 9. Riscos para o agente vigiar

- **FTS5 indisponível** no `rusqlite` bundled → resolver em T0.3 antes de seguir.
- **Licença de dataset ambígua** → não embarcar; registrar e usar alternativa livre.
- **Restauração do terminal na TUI** → sempre usar guarda (RAII) para sair do raw mode mesmo em panic.
- **Vazamento de key** → garantir `.gitignore` e permissões; nunca logar a key.
- **Alucinação da IA** → RAG ancorado no texto local + exigência de citação no prompt + separação texto/interpretação.

---

## 10. Primeira sessão sugerida para o Claude Code

1. Ler `SPEC.md` e este plano.
2. Criar `DATA_SOURCES.md`, `DECISIONS.md`, `PROGRESS.md` vazios com cabeçalho.
3. Executar **T0.1 → T0.5** (Fase 0) e parar no Marco 0 para revisão humana.
4. Reportar: o que foi feito, decisões tomadas, datasets escolhidos e licenças.

> Após o Marco 0 aprovado, seguir para a Fase 1. Não avançar de fase sem
> verificação verde e (quando indicado) revisão humana nos marcos.
