# DATA_SOURCES.md — Bíblia CLI (Phase 0)

> **Data de verificação de todas as fontes abaixo: 2026-06-15** (live-fetch + verificação de licença).
> Cada entrada documenta: URL, formato, licença, decisão de embarque (embed / NÃO embarcar) e a justificativa.
> Regra de ouro: **só embarcamos textos public-domain / CC0 / CC-BY**. Qualquer texto copyrighted (SBB: NVI/ARA/ARC/NTLH; Trinitária: ACF; IBB/JuERP: "Almeida Atualizada/Revisada") é **rejeitado**.

---

## 1. Datasets EMBARCADOS (Phase 0)

### 1.1 EN — King James Version (1769) — **EMBED-FIRST** ✅
- **translationId (db):** `kjv`
- **Abreviação:** KJV
- **URL (exata, funcionando):** https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/KJV.json
- **Formato:** JSON aninhado (objeto), shape scrollmapper `{translation, books:[{name, chapters:[{chapter, verses:[{verse, text}]}]}]}`
- **Tamanho / completude:** ~8,4 MB (content-length 8.395.929), 66 livros, **31.102 versículos** (cânon protestante completo)
- **Licença:** `public-domain` (texto KJV 1769). Empacotamento do repo: MIT (Scrollmapper, 2024) — cobre só o packaging, não o texto. O texto não precisa de outorga porque é domínio público.
- **HTTP:** 200 (verificado live, text/plain servido pelo raw.githubusercontent)
- **Verificado em:** 2026-06-15
- **Decisão:** **EMBARCAR** (versão EN primária).
- **Justificativa:** Fonte single-file mais simples de parsear (chaves nomeadas, inteiros para chapter/verse), texto limpo (apesar do rótulo "with Strong's", que é VERIFICADO FALSO — não há markup), 66 livros / 31.102 versículos sem truncamento. Sem obrigação legal de atribuição.
- **Riscos / notas:** (1) UK Crown Letters Patent sobre a KJV — restrição de impressão **apenas no Reino Unido**, sem efeito sobre embarcar/distribuir o texto em app open-source global; risco prático baixíssimo. (2) Rótulo "translation" menciona Strong's/morfologia — ignorar, conteúdo é texto puro. (3) Aparar espaços em branco à direita em alguns campos (ex.: Rev 22:21). (4) Fazer vendor/mirror do arquivo no repo e fixar (pin) por commit SHA em vez de buscar "master" ao vivo.
- **Alternativa documentada (não escolhida):** **BSB (Berean Standard Bible)** — https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/BSB.json — `cc0` (domínio público desde 2023-04-30, berean.bible/licensing.htm), mesma shape, inglês moderno, 31.102 versículos. Igualmente embarcável; escolhemos KJV como primária por reconhecimento universal e zero strings de licença a honrar. BSB é a substituta recomendada se quisermos inglês moderno.

### 1.2 PT — Almeida 1911 — **EMBED-FIRST** ✅
- **translationId (db):** `alm1911`
- **Abreviação:** ALM1911
- **URL (exata, fixada na release v1.0.0):** https://github.com/damarals/biblias/releases/download/v1.0.0/ALM1911.json
- **Formato:** JSON aninhado (array), shape thiagobodruk `[{abbrev, name, chapters:[[verso, ...], ...]}]`
- **Tamanho / completude:** ~4,04 MB (content-length 4.037.522), 66 livros, 1.189 capítulos, **31.101 versículos**
- **Licença:** `public-domain` (texto Almeida edição 1911, marcado "† domínio público" no README damarals/biblias). Compilação JSON: MIT (damarals/biblias) — cobre só o packaging.
- **HTTP:** 200 (via 302 redirects: github.com → release-assets.githubusercontent.com → Azure blob). Importer DEVE seguir redirects.
- **Verificado em:** 2026-06-15
- **Decisão:** **EMBARCAR** (versão PT primária).
- **Justificativa:** Texto **genuinamente domínio público** (Almeida do séc. XVII, edição 1911, muito além de qualquer prazo de copyright) — **sem obrigação de atribuição**, ideal para Phase 0. NÃO é versão copyrighted da SBB nem da Trinitária. Shape single-file simples (array aninhado).
- **Riscos / notas:** (1) `/releases/latest/` resolve para v1.0.0 hoje — **fixamos a URL na tag v1.0.0** para evitar drift futuro. (2) Ortografia arcaica de 1911 ("creou", "céus", "abysmo") — correto/esperado, não "corrigir". (3) Aparar espaços em branco; normalizar diacríticos numa coluna de busca separada (NFD → remover marcas combinantes → lowercase) para busca PT acento-insensível.
- **Alternativa documentada (não escolhida):** **Bíblia Livre (BLIVRE)** — https://github.com/damarals/biblias/releases/download/v1.0.0/BLIVRE.json — licença **`cc-by` (Brasil), atribuição OBRIGATÓRIA** (© Diego Santos, Mario Sérgio, Marco Teles). Português moderno, 31.102 versículos, mesma shape. Embarcável, MAS exige linha de atribuição visível. **ATENÇÃO: o mirror damarals rotula BLIVRE erradamente como "† domínio público" — isto é FALSO; os termos vinculantes upstream são CC-BY.** Escolhemos ALM1911 por ser PD puro (zero strings de licença). Se quisermos ortografia moderna, usar BLIVRE **com** a atribuição: "Bíblia Livre (BLIVRE), © Diego Santos, Mario Sérgio e Marco Teles — CC-BY (Brasil), fonte: github.com/blivre/BibliaLivre".

### 1.3 XREF — OpenBible.info Cross References — **EMBED-FIRST (Phase 2)** ✅
- **Nome:** OpenBible.info Bible Cross References (derivado do Treasury of Scripture Knowledge)
- **URL (exata, funcionando):** https://a.openbible.info/data/cross-references.zip
- **Formato:** ZIP (deflate) contendo um único TSV `cross_references.txt` (3 colunas: From Verse, To Verse, Votes)
- **Tamanho / completude:** ~8,3 MB descompactado (8.301.303 bytes), ~1,98 MB zipado; 344.800 linhas = 1 cabeçalho + **344.799 referências cruzadas**; 0 versículos de Escritura
- **Licença:** `cc-by` — **ATRIBUIÇÃO OBRIGATÓRIA**. O TSK subjacente é domínio público; o índice compilado/votado da OpenBible é a camada CC-BY.
- **HTTP:** 200 (verificado live)
- **Verificado em:** 2026-06-15
- **Decisão:** **EMBARCAR** como camada de referências cruzadas (usado na Phase 2, documentado agora). NÃO substitui um texto bíblico — contém zero Escritura.
- **Atribuição exigida (string a exibir na app):** `Cross references courtesy of OpenBible.info (CC-BY)` com link para https://www.openbible.info/labs/cross-references/
- **Riscos / notas:** (1) Atribuição CC-BY é vinculante. (2) Versão da licença não fixada na página canônica — tratar como CC-BY genérico. (3) Votos incluem negativos (refs disputadas, mínimo ~ -86) — usar threshold configurável. (4) Refs em versificação ESV/protestante — drift menor contra KJV/Almeida (ambas 66 livros protestante). (5) Coluna "To Verse" pode ser RANGE (`Book.Ch.V-Book.Ch.V`) — expandir no parse.
- **Mirror alternativo (backup, git-pinnable):** https://raw.githubusercontent.com/scrollmapper/bible_databases/master/sources/extras/cross_references.txt — TSV plano não compactado, mesma schema, CC-BY (atribuir OpenBible, NÃO scrollmapper). Snapshot 2024-11-04 com contagens de voto ligeiramente desatualizadas; fixar commit SHA. Use a fonte direta OpenBible para votos mais frescos; use o mirror para uma URL raw estável de um único curl.

---

## 2. Datasets CONSIDERADOS e REJEITADOS (com motivo)

### 2.1 EN — rejeitados ou backup
| Fonte | URL | Licença | Decisão | Motivo |
|---|---|---|---|---|
| KJV (getBible v2) | https://api.getbible.net/v2/kjv.json | ambíguo (rótulo GPL) | **NÃO embarcar** (no máx. backup via API runtime) | Metadados autoritativos da getBible declaram `distribution_license="GPL"` (módulo CrossWire KJV2003); getBible pede para NÃO redistribuir o JSON raw. Texto 1769 é PD, mas o artefato carrega rótulo GPL + condições de redistribuição — incompatível/ambíguo para embed permissivo. Preferir o KJV scrollmapper. |
| WEB (World English Bible) — getBible | https://api.getbible.net/v2/web.json | public-domain (texto) | backup | Texto PD, mas é dependência de CDN de terceiro (getBible), não github raw; marca registrada no nome "World English Bible" (texto modificado não pode usar o nome). Embarcável em substância; usar como backup/inglês moderno alternativo. |
| ASV (American Standard Version 1901) — scrollmapper | https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/ASV.json | public-domain | embarcável (não escolhida) | Gold-standard PD, mesma shape do KJV. Backup EN sólido; KJV escolhida por reconhecimento. |
| YLT (Young's Literal Translation 1898) — scrollmapper | https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/YLT.json | public-domain | backup | PD, mas embute tags `<FI>...<Fi>` (palavras supridas pelo tradutor) — exige strip/render. Útil como versão literal adicional. |
| KJV/ASV (bibleapi-bibles-json) | https://raw.githubusercontent.com/bibleapi/bibleapi-bibles-json/master/kjv.json | public-domain (texto); repo SEM LICENSE | backup | Texto PD, mas shape de array posicional `{"field":[id, book_num, chapter, verse, text]}` com id BBCCCVVV — exige tabela de book-number e parsing de id, mais trabalhoso. Repo sem arquivo de licença. Mesmo repo traz arquivos russos copyrighted (nrt/crtb) — NÃO embarcar esses. |
| aruljohn/Bible-kjv | https://raw.githubusercontent.com/aruljohn/Bible-kjv/master/Genesis.json | public-domain | backup | Texto PD, JSON limpo, MAS um arquivo por livro (66 arquivos + Books.json) com mapeamento nome-com-espaço → nome-sem-espaço (Song of Solomon → SongofSolomon.json, senão 404) e chapter/verse como STRINGS. Mais complexo que o single-file scrollmapper. Bus-factor de mantenedor único. |
| wldeh/bible-api (en-kjv) | https://raw.githubusercontent.com/wldeh/bible-api/main/bibles/en-kjv/books/genesis/chapters/1/verses/1.json | public-domain (só en-kjv) | backup (breadth de idiomas) | en-kjv é PD, mas shape de um arquivo por versículo (~31k+ requests para crawl). Repo tem milhares de versões com copyright heterogêneo (muitas `copyright:""` = DESCONHECIDO, algumas copyrighted). Usar só para idiomas não cobertos, re-verificando licença por versão. |
| scrollmapper AKJV (e bulk multi-translation) | https://raw.githubusercontent.com/scrollmapper/bible_databases/master/formats/json/AKJV.json | ambíguo (AKJV = "Copyrighted; Free non-commercial distribution") | **NÃO embarcar (AKJV)** | Licença é POR-tradução, não repo-wide. AKJV é "non-commercial" = NÃO free-libre = NÃO embarcável. NÃO fazer bulk import cego. O repo É bom para traduções PD/CC0 específicas (ASV, BBE, BSB) lendo o campo `license` de cada uma. |

### 2.2 PT — **NÃO EMBARCAR (copyrighted / ambíguo)** 🚫
> Esta seção existe para impedir embarque acidental. **Nenhuma destas pode ser embarcada.**

| Fonte | URL | Licença real | Decisão | Motivo |
|---|---|---|---|---|
| **JFAA "Almeida Atualizada"** (damarals) | https://github.com/damarals/biblias/releases/latest/download/JFAA.json | **copyrighted** | **NÃO embarcar** 🚫 | Alegação de PD é FALSA: o README damarals NÃO marca JFAA com "†". É a "Almeida Revisada segundo os Melhores Textos" (IBB/JuERP, 1956/1967), **ainda sob copyright** (direitos IBB/JuERP), apenas não fiscalizado. Removida do SWORD Project e do Wikisource por violação de copyright. Armadilha da linhagem thiagobodruk/pt_aa. |
| **open-bibles por-almeida.usfx.xml** | https://raw.githubusercontent.com/seven1m/open-bibles/master/por-almeida.usfx.xml | **copyrighted** | **NÃO embarcar** 🚫 | Rótulo "Public Domain" do README é asserção não verificada de agregador. Fingerprint textual (Jo 3:16 "unigênito", Jo 1:14 "unigênito do Pai") = "Almeida Atualizada"/JuERP "Revisada Segundo os Melhores Textos" (1948/1967), revisão do séc. XX. Mesmo texto "AA" removido do CrossWire/SWORD por violação. Sem declaração de direitos no arquivo. |
| **thiagobodruk aa.json / acf.json / nvi.json** | https://raw.githubusercontent.com/thiagobodruk/biblia/master/json/aa.json | **copyrighted** + wrapper CC BY-NC | **NÃO embarcar** 🚫 | README do repo declara: textos são propriedade da Sociedade Bíblica Internacional (NVI), Sociedade Bíblica Trinitariana (ACF) e Imprensa Bíblica Brasileira (AA), "todos os direitos reservados". Wrapper CC BY-NC (não-comercial) já proíbe redistribuição livre. Armadilha: o "aa" NÃO é Almeida PD — é a "Almeida Revisada IBB" copyrighted. |
| **gratis-bible pt/port.xml** | https://raw.githubusercontent.com/gratis-bible/bible/master/pt/port.xml | ambíguo (rights tag VAZIO) | **NÃO embarcar (desta URL)** 🚫 | Texto provavelmente PD (Unbound Bible Almeida Atualizada), MAS o repo dá ZERO evidência de licença (rights tag vazio; LICENSE diz que a licença está no rights tag de cada XML, que está vazio). Não há base legal de PD nesta fonte. |
| **gratis-bible pt/acf.xml** | https://raw.githubusercontent.com/gratis-bible/bible/master/pt/acf.xml | **copyrighted** (Trinitária) | **NÃO embarcar** 🚫 | ACF © Sociedade Bíblica Trinitária (1994/1995). Copyright firme. Rejeitar permanentemente. |
| **getBible almeida.json** | https://api.getbible.net/v2/almeida.json | free-libre (texto PD) mas artefato rotulado GPL | backup (não embed-first) | Texto Almeida 1911 é PD, mas o arquivo auto-rotula `distribution_license="GPL"` (confirmado no arquivo e em translations.json). GPL sobre texto PD é legalmente fraco, mas injeta ambiguidade num embed permissivo. Preferir damarals ALM1911 (MIT + PD). Usar só como fallback. |

> **Versões copyrighted da SBB e Trinitária — NUNCA embarcar:** NVI (© Sociedade Bíblica Internacional/Biblica), ARA / ARC / NTLH (© Sociedade Bíblica do Brasil), ACF (© Sociedade Bíblica Trinitária), e qualquer "Almeida Atualizada/Revisada" da IBB/JuERP. `embeddable = false` para todas. Para PT genuinamente livre, usar **Almeida 1911 (PD)** ou **Bíblia Livre (CC-BY, com atribuição)**.

### 2.3 XREF — alternativa documentada
| Fonte | URL | Licença | Decisão | Motivo |
|---|---|---|---|---|
| scrollmapper (mirror TSK) | https://raw.githubusercontent.com/scrollmapper/bible_databases/master/sources/extras/cross_references.txt | cc-by | backup / mirror | Mesmos dados OpenBible (TSV não compactado), git-pinnable com um único curl. Snapshot 2024-11-04 (votos um pouco desatualizados). Atribuir **OpenBible.info**, não scrollmapper. Fixar commit SHA. Preferir a fonte direta OpenBible para votos frescos. |

---

## 3. Resumo das decisões de Phase 0

| Slot | Escolha | translationId | Licença | Atribuição obrigatória? |
|---|---|---|---|---|
| EN (embed) | King James Version (1769) | `kjv` | public-domain | Não |
| PT (embed) | Almeida 1911 | `alm1911` | public-domain | Não |
| XREF (embed, Phase 2) | OpenBible.info Cross References | — | cc-by | **Sim** — "Cross references courtesy of OpenBible.info (CC-BY)" |

**Substitutas aprovadas:** EN → BSB (`bsb`, CC0, sem atribuição) ou ASV (`asv`, PD). PT → Bíblia Livre (`blivre`, CC-BY, **com atribuição obrigatória**).

**Procedimento de embarque:** fazer vendor/mirror de cada arquivo escolhido no repositório, fixar por commit SHA / tag de release (já feito para ALM1911 = v1.0.0), e não buscar "master" ao vivo em runtime.

---

## 4. Conectores de versões protegidas (Phase 6) — **NUNCA embarcadas**

Versões sob copyright (ARA/ARC/NTLH/NVI/ACF em PT; ESV/NIV/NASB/CSB/NLT em EN)
**não são embarcadas nem cacheadas em massa**. São lidas **ao vivo** sob a
credencial do próprio usuário, que aceita os termos de cada API. A camada de
fontes (`source::BibleSource`) isola essa fronteira: conectores têm
`is_embeddable() == false`, `license = Proprietary`, e a `Passage` devolvida é
efêmera (exibição/citação pessoal).

| Conector | API | Endpoint | Auth | Texto em | Observações |
|---|---|---|---|---|---|
| `apibible` | [API.Bible](https://scripture.api.bible) | `GET /v1/bibles/{bibleId}/passages/{passageId}` (USFM, ex.: `JHN.3.16`) | header `api-key` | `data.content` | Cobre ARA/NVI/etc. conforme o `bibleId` da conta do usuário. Texto puro (sem números/títulos/notas). |
| `esv` | [ESV API (Crossway)](https://api.esv.org) | `GET /v3/passage/text/?q=<ref>` | header `Authorization: Token <key>` | `passages[0]` | Mantemos `include-short-copyright=true` para preservar a atribuição "(ESV)" exigida; limite de citação ~500 versículos/≤25% da obra é responsabilidade do usuário. |

**Configuração (BYOK):** `biblia config connector add <slug> --kind <apibible|esv> [--bible-id <id>] --name "..." --abbrev <ABBR> --lang <pt|en>` mapeia um slug (ex.: `ara`) para a fonte; a chave vai no cofre `secrets.toml` (0600, fora do git) via `biblia config set-key <apibible|esv> <chave>`. Sem chave → indisponível com mensagem clara, **sem** chamada de rede. A busca full-text (`search`) é só local: conectores retornam `Unsupported`.
