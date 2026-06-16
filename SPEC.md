# The Light — Documento de Spec & Roadmap

> Leitor de Bíblia hackeável para terminal, com estudo exegético assistido por IA
> (lente denominacional configurável), dados locais e modelo *bring-your-own-key*.
> Stack: **Rust**. Conteúdo inicial: **bilíngue PT + EN**.

Versão do documento: 0.1 · Data: 2026-06-15

---

## 1. Visão

Um aplicativo de linha de comando (CLI + TUI) para ler, pesquisar, anotar e
estudar a Bíblia diretamente do terminal. Funciona 100% offline com versões de
domínio público, e oferece uma camada opcional de IA — ativada quando o usuário
fornece a própria chave de API — para estudo exegético aprofundado a partir de
uma perspectiva teológica escolhida (Batista, Presbiteriana, Luterana,
Pentecostal, etc.).

Os dados do usuário (notas, marcações, planos de leitura) vivem em arquivos
locais legíveis e versionáveis em git. Sem servidor obrigatório, sem lock-in,
com privacidade por padrão.

### Princípios de design

1. **Offline-first.** Tudo essencial funciona sem internet e sem IA.
2. **Bring-your-own-key (BYOK).** A IA é opcional; o usuário paga seu próprio uso.
3. **Dados do usuário são do usuário.** Notas e marcações em arquivos abertos, locais, versionáveis.
4. **Licença em primeiro lugar.** Só embarcamos versões livres; versões protegidas só via conector opt-in com credenciais do próprio usuário.
5. **Hackeável.** Configuração em texto, fontes plugáveis, prompts de IA editáveis.
6. **Texto vs. interpretação sempre distinguíveis.** A saída de IA cita versículos e marca claramente o que é leitura interpretativa.

---

## 2. O gap que fechamos

Pesquisa de mercado (jun/2026) mostra três categorias maduras, mas **isoladas**:

| Categoria | Exemplos | Limitação |
|---|---|---|
| CLIs/TUIs de leitura | bible-tui, Bible.JS, bible-cli, pybible-cli | Só leem. Sem IA, sem lente teológica, notas limitadas. |
| APIs de texto | API.Bible, Free Use Bible API, wldeh, bible-edge (NVI) | São fontes de dados, não um produto de estudo. |
| Apps de IA / estudo | Spirit Speak AI, Aura, Navigate The Way, Biblical-AI | Web/mobile fechados. Sem terminal, sem dados locais, sem BYOK real. |

**Nenhuma ferramenta combina as três coisas:** leitura nativa de terminal +
estudo exegético com perspectiva denominacional explícita + BYOK com dados
locais. É exatamente esse cruzamento que ocupamos.

Frase-resumo do posicionamento: *"um leitor de Bíblia hackeável de terminal, com
camada de IA opcional e lente teológica configurável, que respeita licenças e roda
com as chaves do próprio usuário."*

---

## 3. A restrição que define a arquitetura: licenciamento

Esta é a decisão de engenharia mais importante. **O modelo de fontes precisa
separar o que pode ser embarcado do que não pode.**

### Pode ser embarcado e redistribuído (domínio público / licença livre)

- **Inglês:** KJV, ASV (1901), YLT, **BSB/Berean** (dedicada ao domínio público em 2023), WEB.
- **Português:** Almeida 1911 / João Ferreira de Almeida em domínio público, e datasets livres (ex.: thiagobodruk/biblia). Verificar versão a versão.

### NÃO pode ser embarcado — somente via conector com credencial do usuário

- **Inglês:** ESV, NIV, NASB, CSB, NLT (uso comercial exige licença; citação limitada a ~500 versículos e ≤25% da obra).
- **Português:** **ARA, ARC, NTLH, NVI** — a Sociedade Bíblica do Brasil (SBB) **não autoriza redistribuição**. Estas só aparecem se o usuário conectar uma API (ex.: API.Bible) com a própria conta, aceitando os termos dela.

### Consequência arquitetural

O app é **bring-your-own-source**: versões livres vêm embutidas; versões
protegidas são buscadas em tempo real via conector opt-in, nunca empacotadas no
binário nem cacheadas em massa. A camada de fontes (§5.2) é abstrata justamente
para isolar essa fronteira legal.

---

## 4. Por que Rust

- **Binário único, sem runtime.** Distribuição trivial (`brew`, `cargo install`, releases no GitHub) — sem pedir Python/Node ao usuário.
- **Performance.** Busca full-text e renderização de TUI instantâneas mesmo em bases grandes.
- **Ecossistema TUI maduro.** `ratatui` + `crossterm` para a interface; `clap` para CLI.
- **SQLite embutido** via `rusqlite` (com FTS5 para busca).
- **Cliente HTTP** (`reqwest`) para conectores de API e LLM, com `tokio` para async.

### Crates candidatas

| Função | Crate |
|---|---|
| Parser de CLI | `clap` (derive) |
| TUI | `ratatui` + `crossterm` |
| Armazenamento | `rusqlite` (SQLite + FTS5) |
| HTTP / API / LLM | `reqwest` + `tokio` |
| Serialização config/dados | `serde` + `toml` + `serde_json` |
| Markdown (notas/render) | `pulldown-cmark` |
| Diretórios de config | `directories` (XDG) |
| Cores / tema | `ratatui` styles |

---

## 5. Arquitetura

### 5.1 Visão em camadas

```
┌─────────────────────────────────────────────────────────┐
│  Interface:  CLI (clap)   |   TUI (ratatui)              │
├─────────────────────────────────────────────────────────┤
│  Núcleo de aplicação                                     │
│   leitura · busca · navegação · referências cruzadas     │
│   notas · highlights · planos de leitura                 │
├──────────────────┬───────────────────┬──────────────────┤
│  Camada de FONTES│  Dados do USUÁRIO  │  Camada de IA     │
│  (texto bíblico) │  (notas/marcações) │  (opcional, BYOK) │
│  ┌─────────────┐ │  arquivos locais   │  ┌──────────────┐ │
│  │ Embarcada   │ │  Markdown / JSON   │  │ Provedores   │ │
│  │ (SQLite)    │ │  versionáveis      │  │ OpenAI/      │ │
│  ├─────────────┤ │  em git            │  │ Anthropic/   │ │
│  │ Conector API│ │                    │  │ local (Ollama)│ │
│  │ (opt-in)    │ │                    │  └──────────────┘ │
│  └─────────────┘ │                    │                   │
└──────────────────┴───────────────────┴───────────────────┘
```

### 5.2 Camada de fontes (trait `BibleSource`)

Abstração única para qualquer origem de texto. Implementações:

- `EmbeddedSource` — lê do SQLite local (versões livres).
- `ApiBibleSource` — busca via API.Bible com key do usuário (versões protegidas).
- `EsvApiSource` — conector específico ESV.
- Futuro: `FileSource` para módulos importados pelo usuário.

```rust
trait BibleSource {
    fn translations(&self) -> Vec<Translation>;
    fn passage(&self, ref_: &Reference, tr: &TranslationId) -> Result<Passage>;
    fn search(&self, query: &str, tr: &TranslationId) -> Result<Vec<SearchHit>>;
    fn is_embeddable(&self) -> bool; // governa cache/redistribuição
}
```

### 5.3 Modelo de dados (SQLite)

**Texto bíblico (read-only, embarcado):**

```
translations(id, abbrev, name, language, license, embeddable)
books(id, translation_id, name, abbrev, order, testament)
verses(id, translation_id, book_id, chapter, verse, text)
verses_fts  -- índice FTS5 sobre verses.text para busca
cross_references(from_book, from_ch, from_v, to_book, to_ch, to_v, votes)
```

Referências cruzadas iniciais a partir do dataset **Treasury of Scripture
Knowledge** (domínio público, ~340 mil cross-refs).

**Dados do usuário (read-write, em arquivos):**

```
~/.config/light/config.toml            # versões, fonte, tamanho, tema, perspectiva
~/.local/share/light/notes/            # uma nota .md por versículo/intervalo
~/.local/share/light/highlights.json   # {ref, cor, tag}
~/.local/share/light/reading-plans/    # planos + progresso
~/.local/share/light/studies/          # estudos de IA salvos (.md)
```

Tudo em texto aberto → o usuário pode versionar a pasta inteira em git e
sincronizar entre máquinas.

### 5.4 Modelo de referência

Parser robusto de referências aceitando PT e EN:
`Jo 3.16`, `John 3:16`, `Gn 1.1-3`, `Sl 23`, `1Co 13.4-7`, intervalos e listas.
Tabela de aliases de livros por idioma.

---

## 6. Funcionalidades

### 6.1 Núcleo (offline, sem IA)

- **Múltiplas versões** e leitura em paralelo (ex.: ARA livre + KJV lado a lado).
- **Leitura** por capítulo/passagem com viewport rolável na TUI.
- **Busca** full-text (FTS5), por palavra/frase, com filtro por versão e livro.
- **Highlights** com cores e tags.
- **Notas** associadas a versículo ou intervalo (Markdown).
- **Referências cruzadas** — saltar entre versículos relacionados (TSK).
- **Planos de leitura** (cronológico, anual, temático) com acompanhamento de progresso e — opcionalmente — integração de calendário.
- **Fonte e tamanho** — escolha de fonte/tamanho onde o terminal permitir (via sequências ANSI/ajuste de layout) e temas de cor.
- **Export** de notas/estudos para Markdown/PDF.

### 6.2 Camada de IA (opcional, BYOK)

- **Estudo exegético aprofundado** de uma passagem: contexto histórico-literário, estrutura, palavras-chave (grego/hebraico), aplicação.
- **Lente denominacional** selecionável — a perspectiva (Batista / Presbiteriana / Luterana / Pentecostal / Católica / Ortodoxa / "comparar perspectivas") entra como *system prompt*. Saída sempre cita os versículos e separa **texto** de **interpretação**, sinalizando quando há divergência entre tradições.
- **Comparar perspectivas** — mesma passagem sob duas ou mais lentes, lado a lado.
- **Perguntas livres** ancoradas em referências (RAG sobre o texto local para reduzir alucinação).
- **Provedores plugáveis:** OpenAI, Anthropic, ou modelo local (Ollama) — configurável; a chave nunca sai da máquina exceto para o provedor escolhido.

### 6.3 Comandos (esboço de CLI)

```
light read John 3:16 --version kjv,ara
light search "graça" --version ara
light highlight "Jo 3.16" --color yellow --tag salvação
light note add "Sl 23.1" "O Senhor é o meu pastor..."
light xref "Rm 3.23"
light plan start chronological --year 2026
light plan today
light study "Ef 2.8-9" --lens presbiteriana      # requer key
light study compare "Tg 2.24" --lens batista,luterana
light config set font-size 16
light tui                                          # abre a interface completa
```

---

## 7. Distribuição e modelo de uso aberto

- **Open source** (sugestão: licença MIT ou Apache-2.0 para o código; dados bíblicos sob suas próprias licenças).
- **Instalação:** `cargo install --git https://github.com/butkeraites/the-light the-light-cli`, Homebrew, e binários pré-compilados (Linux/macOS/Windows) via GitHub Releases.
- **Configuração de keys:** `light config set-key openai sk-...` grava em local seguro do SO; nunca commitada.
- **Sem backend obrigatório.** O projeto pode ser usado por qualquer pessoa que clone/instale e traga suas próprias fontes e chaves — exatamente o "uso aberto" desejado.

---

## 8. Roadmap por fases

### Fase 0 — Fundação (semana 1–2)
Scaffolding Rust, `clap`, esquema SQLite, importador de uma versão livre PT e uma EN, parser de referências, comando `read`.

### Fase 1 — Leitura & busca (semana 3–4)
Múltiplas versões, leitura paralela, busca FTS5, configuração (`config.toml`), tema/cores. CLI utilizável.

### Fase 2 — Estudo pessoal (semana 5–6)
Highlights, notas em Markdown, referências cruzadas (import TSK), export.

### Fase 3 — TUI (semana 7–9)
Interface `ratatui` completa: navegação, viewport, painéis de notas/xref, busca interativa, escolha de fonte/tamanho/tema.

### Fase 4 — Planos de leitura (semana 10)
Planos cronológico/anual/temático, progresso, lembretes/calendário.

### Fase 5 — Camada de IA / BYOK (semana 11–13)
Abstração de provedor, gestão de keys, prompts de lente denominacional, estudo exegético, comparar perspectivas, RAG sobre texto local.

### Fase 6 — Conectores de versões protegidas (semana 14)
`ApiBibleSource` e `EsvApiSource` opt-in para ARA/NVI/ESV via key do usuário.

### Fase 7 — Polimento & lançamento aberto (semana 15+)
Empacotamento, docs, Homebrew/crates.io, prompts editáveis pela comunidade.

---

## 9. Riscos e mitigações

| Risco | Mitigação |
|---|---|
| Licença de versões PT (ARA/NVI) | Nunca embarcar; só via conector com key do usuário. Embutir apenas versões livres. |
| Alucinação da IA em temas teológicos | RAG ancorado no texto local; sempre citar versículos; separar texto de interpretação; rótulo de "lente". |
| Viés denominacional indevido | Lente sempre explícita e escolhida pelo usuário; modo "comparar perspectivas"; transparência de que é a visão *daquela* tradição. |
| Fonte/tamanho no terminal | Suporte depende do emulador; degradar graciosamente; documentar limites. |
| Custo de API para o usuário | BYOK + opção de modelo local (Ollama); mostrar estimativa de tokens. |

---

## 10. Próximas decisões em aberto

1. Quais versões livres exatas embarcar no PT e no EN (verificar licença individual).
2. Provedores de IA a suportar no MVP (sugestão: Anthropic + OpenAI + Ollama).
3. Nome do binário/projeto e licença do código.
4. Profundidade do parser grego/hebraico (Strong's? interlinear?) — pode ser fase posterior.

---

*Documento de planejamento. As afirmações de licenciamento devem ser confirmadas
versão a versão antes do empacotamento.*
