# Prompts editáveis (lentes denominacionais)

O `biblia study` monta o *system prompt* a partir de uma **lente denominacional**
(a tradição cuja perspectiva você quer) mais uma **profundidade**. Os prompts são
embutidos no binário, mas totalmente **sobrescrevíveis** — esse é um dos pilares
"hackeáveis" do projeto.

## Como sobrescrever

Crie um arquivo `prompts/<lente>.md` no diretório de configuração. Se o arquivo
existir, **seu conteúdo substitui integralmente** o system prompt embutido daquela
lente (a profundidade e o idioma continuam sendo informados pelo orquestrador via
o *user prompt*).

Diretório padrão:

- **Linux:** `~/.config/biblia/prompts/`
- **macOS:** `~/Library/Application Support/biblia/prompts/`
- **Override:** defina `BIBLIA_PROMPTS=/caminho/para/prompts` para apontar para
  outro diretório (útil para versionar seus prompts num repositório).

Exemplo:

```sh
mkdir -p ~/.config/biblia/prompts
$EDITOR ~/.config/biblia/prompts/presbyterian.md
biblia study "Ef 2.8-9" --lens presbiteriana   # usa o seu prompt
```

## Slugs das lentes

O nome do arquivo usa o **slug** em inglês (a flag `--lens` aceita PT e EN):

| Lente | Slug (arquivo) | `--lens` aceita |
|---|---|---|
| Batista | `baptist` | `batista`, `baptist` |
| Presbiteriana / Reformada | `presbyterian` | `presbiteriana`, `reformada`, `presbyterian`, `reformed` |
| Luterana | `lutheran` | `luterana`, `lutheran` |
| Pentecostal | `pentecostal` | `pentecostal` |
| Católica | `catholic` | `católica`, `catolica`, `catholic` |
| Ortodoxa | `orthodox` | `ortodoxa`, `orthodox` |

## Boas práticas para o seu prompt

O estudo é desenhado para **reduzir alucinação** e ser honesto sobre a perspectiva.
Ao escrever um prompt próprio, mantenha as regras que sustentam isso:

1. **Cite os versículos** por referência ao fundamentar afirmações.
2. **Separe o texto bíblico citado da interpretação** — o `biblia` já imprime o
   texto do acervo local numa seção própria; o modelo deve deixar claro o que é
   leitura interpretativa.
3. **Marque a lente** — deixe explícito que é a leitura *daquela* tradição, não a
   única visão cristã; sinalize divergências relevantes entre tradições.
4. **Não invente** versículos, referências, números de Strong ou citações.

Profundidades disponíveis (flag `--depth`): `geral`, `exegetico`, `palavras`.

## Lembrete

A IA é **opt-in e BYOK**. Configure o provedor e a chave antes:

```sh
biblia config set provider anthropic
biblia config set-key anthropic <sua-chave>
```

A chave fica em `secrets.toml` (`0600`, fora do git) e nunca é logada. Para testar
o fluxo sem rede nem chave, use `--provider mock`.
