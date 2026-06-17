# Contexto — The Light

Glossário do domínio (leitor/estudo bíblico). Termos específicos deste projeto,
sharpados conforme decisões de arquitetura se cristalizam. Conceitos gerais de
programação não entram aqui.

## Linguagem

**Referência**:
Uma localização resolvida na Escritura — livro + capítulo + intervalo de
versículos (`Single`, `Range` ou `WholeChapter`).
_Avoid_: citação, endereço, localização.

**Passagem**:
Os versículos (com texto) aos quais uma Referência resolve, vindos de uma
tradução específica. Numerada por versículo para citação fiel.
_Avoid_: trecho, texto, seleção.

**Texto citado** (vs **Interpretação**):
O texto dos versículos vem **verbatim do acervo local**; o modelo produz apenas a
Interpretação. Os dois são mantidos separados (anti-alucinação, ADR-0008).
_Avoid_: misturar resposta e citação num só bloco.

**Contexto RAG** (o bloco montado por `ai::ask_context`):
O contexto **somente local** entregue à IA numa pergunta ancorada — rótulo da
referência + versículos numerados + referências relacionadas. Montado em um único
lugar e idêntico para a CLI (`ask`) e a TUI.
_Avoid_: prompt, janela de contexto, system prompt (são outras coisas).

**Referências relacionadas** (rótulos — `xref::passage_labels`):
Os rótulos formatados das referências cruzadas de **toda a passagem**, agregados e
deduplicados por votos. É o que entra no Contexto RAG, não os `CrossRef` crus.
_Avoid_: refs cruzadas (quando se quer dizer os rótulos já formatados), xref list.
