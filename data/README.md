# `data/` — datasets e importação

A lógica de importação vive em [`xtask`](../xtask) (`cargo run -p xtask -- import`).
Esta pasta guarda apenas os arquivos brutos baixados.

## Layout

- `seed/` — datasets brutos baixados (KJV, Almeida 1911...). **Não versionados**
  (ver `.gitignore`); são recriados sob demanda pelo importador.

## Como gerar o banco

```sh
cargo run -p xtask -- import --version kjv,alm1911 --db data/biblia.sqlite
```

Isso baixa os datasets livres (ver `DATA_SOURCES.md` para URLs/licenças), popula
`translations/books/verses/verses_fts` e valida a contagem de versículos
(KJV = 31.102, Almeida 1911 = 31.101). O comando é idempotente.

> **Licenciamento:** o importador só conhece versões de domínio público
> (`xtask/src/import.rs::SPECS`). Versões protegidas (NVI/ARA/ARC/ESV...) nunca
> são embarcadas — ver `DATA_SOURCES.md`.
