<div align="center">

# ✦ The Light

**A hackable, offline-first Bible reader for your terminal — with an opt-in, bring-your-own-key AI study layer that's grounded in real scholarship, not hallucinations.**

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![CI](https://github.com/butkeraites/the-light/actions/workflows/ci.yml/badge.svg)](https://github.com/butkeraites/the-light/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/butkeraites/the-light?sort=semver)](https://github.com/butkeraites/the-light/releases)
![Platforms](https://img.shields.io/badge/platforms-Linux%20%C2%B7%20macOS%20%C2%B7%20Windows-informational)
![Bible text: PT · EN](https://img.shields.io/badge/Bible%20text-PT%20%C2%B7%20EN-success)

</div>

---

The Light is a single, fast binary that turns your terminal into a serious Bible study workspace. Read and search the Scriptures completely **offline**. Keep your notes and highlights as **plain, git-versionable files** you actually own. And when — and only when — you want it, switch on an AI study layer that does deep exegesis through a **denominational lens you choose**, citing verified Greek and Hebrew, and telling you plainly what is verifiable versus what the model wrote.

No account. No server. No telemetry. Your keys, your data, your machine.

```
┌ ✦ The Light ───────────────────────────────────────  John · KJV ─┐
│ Genesis      │ 16  For God so loved the world,    │ Study ▸ Ef 2  │
│ Exodus       │     that he gave his only          │ Mode  Academic│
│  …           │     begotten Son, that whosoever   │ Lens  Presbyt.│
│ Luke         │     believeth in him should not    │ Depth Exeget. │
│ John       ◀ │     perish, but have everlasting   │───────────────│
│ Acts         │     life.                          │ [a] ask       │
│ Romans       │ 17  For God sent not his Son into  │ [+] deepen    │
│  …           │     the world to condemn the world │ [e] export    │
│ Revelation   │     but that the world through him │ [L] cycle lens│
└──────────────┴────────────────────────────────────┴───────────────┘
 ? help  · / search  · g go  · x refs  · v version  · t theme  · q quit
```

---

## Why The Light?

Today's Bible tooling forces a choice between three worlds that never talk to each other:

| Category | Examples | The limitation |
|---|---|---|
| **Reading CLIs / TUIs** | `bible-tui`, `bible-cli`, `pybible-cli` | They only *read*. No study layer, no theological perspective, limited notes. |
| **Text APIs** | API.Bible, Free Use Bible API | They're data sources, not a study product — you still have to build everything. |
| **AI / study apps** | Closed web & mobile apps | No terminal, no local data, no real BYOK — your study lives on someone else's server. |

**No tool combines all three:** native-terminal reading **+** explicit denominational exegesis **+** bring-your-own-key AI over **local, open data**. That intersection is exactly where The Light lives.

> *"A hackable terminal Bible reader, with an optional AI layer and a configurable theological lens, that respects licenses and runs on the user's own keys."*

---

## Highlights

- 📖 **100% offline core** — reading, search, notes, highlights, cross-references and reading plans need zero network and zero AI.
- 🌍 **Bilingual PT/EN** — natural reference parsing for both (`John 3:16`, `Jo 3.16`, `Gn 1.1-3`), multiple versions side by side.
- 🧠 **AI study that's grounded & cited** — exact verse text always comes from the local store; original-language data is injected as *constraints*, and fabricated citations are stripped automatically.
- 🎓 **4 study modes × 6 denominational lenses × 3 depths** — Academic / Devotional / Introductory / Sermon, through a Baptist, Presbyterian, Lutheran, Pentecostal, Catholic or Orthodox lens.
- 🔡 **Verified Greek & Hebrew** — STEPBible original-language tokens with Strong's numbers and brief lexicons, joined per verse.
- 🔗 **~344,000 cross-references** — OpenBible.info (Treasury of Scripture Knowledge), vote-ranked.
- 📝 **Academic export** — SBL footnotes + bibliography to Markdown, and on to PDF/DOCX via Pandoc, with a machine-readable citations sidecar.
- 🗓 **Reading plans** — annual / NT / gospels, with progress tracking and `.ics` calendar export.
- 🖱 **A real TUI** — clickable, mouse-selectable, copy-with-citation, themeable (dark / light / no-color).
- 🔒 **Zero telemetry, true BYOK** — keys live in a `secrets.toml` at mode `0600`, out of git, never logged or echoed.
- 📦 **One static binary** — bundled SQLite, `rustls` TLS, no system OpenSSL, nothing to install around it.

---

## Quick start

```sh
# 1. Install — prebuilt binary (recommended): grab your platform's archive from
#    https://github.com/butkeraites/the-light/releases  (each ships a .sha256)

# …or build/install from source with cargo (installs the `light` binary):
cargo install --git https://github.com/butkeraites/the-light the-light-cli
#    or, from a local clone:
cargo install --path crates/the-light-cli

# 2. Build the local database once (free, public-domain versions):
cargo run -p xtask -- import --version kjv,alm1911 --db data/biblia.sqlite

# 3. Read, search, and open the full terminal UI:
light read "John 3:16" --version kjv,alm1911
light search "grace" --book Romans
light tui
```

> **Requirements:** Rust **1.85+** to build (MSRV). SQLite is bundled and TLS is `rustls`, so there's no system OpenSSL dependency. Colors disable automatically in pipes, with `--plain`, `NO_COLOR`, or `theme = none`.

---

## Features

### 📖 Read & search

Reference parsing understands both Portuguese and English, single verses, ranges, whole chapters, and multiple references at once — and renders any number of versions side by side:

```sh
light read "Gn 1.1-3" --version alm1911
light read "John 3:16" --version kjv,alm1911     # two columns, aligned
```

Full-text search is **accent-insensitive**, **BM25-ranked**, highlighted in the terminal, and filterable by book — powered by SQLite FTS5:

```sh
light search "graça" --version alm1911 --book Romanos
```

### 📝 Personal study, in plain files

Everything you create is yours, in open formats you can read, grep, and version with git — under your OS data directory (or `LIGHT_DATA_DIR`):

```sh
light highlight add "Jo 3.16" --color yellow --tag salvação
light note add "Jo 3.16" "The **central** verse."
light xref "Rm 3.23"                              # cross-references (OpenBible)
light plan start annual --year 2026               # reading plan with progress
light plan ics --output plan.ics                  # import into any calendar
light export notes --format md --output notes.md  # → Markdown (or PDF via pandoc)
```

Notes are Markdown (one file each), highlights are JSON, plans track your daily progress — no proprietary database, no lock-in.

> Cross-references are a one-time import: `cargo run -p xtask -- import-xref --db data/biblia.sqlite`.

### 🖱 The terminal UI

`light tui` opens a responsive, mouse-aware interface: a books sidebar, a numbered-verse reader, and a study/AI panel. **Click** to navigate books, verses, and menus; **drag** to select text in the reader and copy it **with its citation** attached. The layout degrades gracefully on narrow terminals, and `t` cycles dark → light → no-color (which uses bold/reverse/underline for maximum terminal compatibility).

```
  Ask · Ef 2.8-9 · Academic / Presbyterian ─────────────────────────┐
  ▸ Round 2 of 3 — narrow your focus:                                │
                                                                     │
    What do you want to go deeper on?                                │
     1  The meaning of "grace" (χάρις) in context                    │
     2  Faith vs. works — the structure of the argument             │
     3  How this passage reads under covenant theology              │
     4  Write my own focus…                                          │
                                                                     │
   ↑↓ choose · 1-9 quick-pick · c custom · Esc cancel                │
  ───────────────────────────────────────────────────────────────────┘
```

**Key bindings (global):**

| Key | Action | Key | Action |
|---|---|---|---|
| `?` | Help overlay | `a` | Ask AI / continue chat |
| `Tab` | Switch focus (books ⇄ reader) | `s` | Saved sessions & studies |
| `/` | Full-text search | `m` | Study mode & lens picker |
| `g` | Go to reference | `d` | Install scholarly data |
| `v` | Cycle version | `c` | AI provider / key settings |
| `x` | Cross-references | `t` | Cycle theme |
| `n` / `p` | Next / previous chapter | `q` | Quit |

Inside an active study: `a` follow-up question · `+` deepen (Overview → Exegetical → Word study) · `e` export · `L` cycle lens.

### 🧠 AI study — opt-in, BYOK

The AI layer is entirely optional and never runs unless you turn it on with your own key. Providers: **Anthropic**, **OpenAI**, **Ollama** (local, no key) — plus a `mock` provider for offline demos.

```sh
light config set provider anthropic
light config set-key anthropic sk-ant-...          # → secrets.toml (0600), out of git

# Deep exegetical study through a chosen lens:
light study "Ef 2.8-9" --lens presbiteriana --depth exegetico

# Compare two traditions side by side:
light study "Ef 2.8-9" --lens batista,luterana

# A free-form question, anchored to a passage (lightweight RAG):
light ask "How does Paul define grace?" --ref "Rm 3"

# No key? Try the offline demo provider:
light study "Jo 1.1" --lens batista --provider mock
```

A study composes three independent dimensions:

- **Mode** — `Academic` (rigorous, with apparatus) · `Devotional` · `Introductory` · `Sermon` (homiletic outline).
- **Lens** — `Baptist` · `Presbyterian` · `Lutheran` · `Pentecostal` · `Catholic` · `Orthodox`. Each lens's full hermeneutical framework is described to the model, so it *applies* the tradition rather than guessing it.
- **Depth** — `Overview` · `Exegetical` · `WordStudy` (original languages).

To ground studies in the original languages, import the scholarly data once: `cargo run -p xtask -- import-scholarly --db data/biblia.sqlite` (or press `d` in the TUI). In the TUI, studies start with a **3-round scope-refinement wizard** that narrows what you actually want to study, and every conversation is saved as a **persistent, resumable multi-turn session** — so a follow-up question next week keeps the same hermeneutical voice. Lens prompts are just Markdown: drop a `prompts/<lens>.md` in your config dir to override any of them (see [`docs/PROMPTS.md`](docs/PROMPTS.md)).

### ⭐ Grounded, not hallucinated

This is the part that makes AI study *trustworthy* — and the reason The Light exists. Instead of asking a model to "talk about a verse," the app surrounds it with a scaffold of verified data and then **validates what comes back**:

- **Exact text comes from the database, never the model.** Cited verses are pulled from the local store, numbered, and clearly separated from interpretation.
- **Original languages are injected as constraints.** For Academic and Sermon modes, the verified Greek/Hebrew lexicon for the passage (STEPBible Strong's data) is placed in the prompt with explicit anchors like `[V:G5485]` and the instruction to use *only* those numbers and senses — and to declare absence when there's no data, rather than invent.
- **Citations are validated deterministically.** After generation, the app scans for `[V:…]` (lexicon) and `[W:…]` (web) anchors, rewrites the valid ones into proper footnotes, and **silently strips any fabricated or out-of-range citation** — the model cannot smuggle in a Strong's number or a source that doesn't exist.
- **Web research is pre-fetched and opt-in.** With `--research`, the app fetches real snippets (Wikipedia keyless, or Tavily with your key) *before* the model sees them, shows you the query that leaves your machine, and logs it — so URLs can be cited but never invented.
- **A provenance footer separates the three layers** of every academic study: what is **verifiable** (local text + STEPBible data), what was **retrieved from the web** (with snippet and access date), and what was **generated by AI** (named provider/model, "may contain errors — always verify").

```
Ephesians 2.8-9 — Exegetical Study (Presbyterian)

  8  For by grace are ye saved through faith; and that not of
     yourselves: it is the gift of God:
  9  Not of works, lest any man should boast.

  Lexical analysis
  The noun rendered "grace" is χάρις (cháris)[^G5485], denoting
  unmerited favor freely given …

  ── Notes ───────────────────────────────────────────────────
  [^G5485]  STEP Bible, Translators Brief lexicon of Extended
            Strongs for Greek (TBESG), s.v. "cháris (G5485)."

  ── Provenance ──────────────────────────────────────────────
  Verifiable   Bible text (local); lexical data © STEP Bible (CC BY 4.0)
  AI-generated Analysis by anthropic/claude-opus-4-8 under the
               Presbyterian lens — may contain errors; always verify.
```

### 🎓 Academic export

`--academic` prints a scholarly paper (SBL-style footnotes + bibliography); `--export paper.md|.pdf|.docx` writes it out (PDF/DOCX via Pandoc, with YAML front-matter). `--save` keeps the study in `studies/` alongside a round-trippable `.citations.json` sidecar.

```sh
light study "Ef 2.8-9" --lens presbiteriana --academic --export study.pdf
```

### 🔌 Protected versions via connectors

Copyrighted versions (ARA, NVI, ESV, …) are **never embedded or bulk-cached**. You connect an API with **your own credentials**, and they're fetched live, on demand:

```sh
light config connector add ara --kind apibible --bible-id <id> --abbrev ARA --lang pt
light config set-key apibible <your-api.bible-key>
light read "Jo 3.16" --version kjv,ara          # free + protected, side by side
```

No key → the version is simply unavailable, with a clear message and **no network call**.

---

## Privacy & security

**Zero telemetry.** `light` never collects, sends, or logs anything by default. The *only* time it touches the network is when **you** explicitly ask: an AI study/question (goes solely to the provider you chose) or reading a protected version (goes solely to the API you configured, with your key). Everything else — reading, search, notes, plans — is 100% local. Keys live in `secrets.toml` (`0600`, out of git, or `LIGHT_SECRETS`) and are never logged or echoed.

---

## Architecture

A small, well-factored Rust 2021 workspace:

```
crates/
  the-light-core/   # pure logic: model, reference parsing, SQLite store,
                    # sources, the AI layer, and user data
  the-light-cli/    # the `light` binary (clap) — 11 subcommands
  the-light-tui/    # the ratatui terminal interface
xtask/              # dataset import tasks (cargo run -p xtask)
data/               # raw open datasets — see DATA_SOURCES.md
```

Bundled SQLite (`rusqlite`), `rustls` TLS (no OpenSSL), blocking HTTP via `reqwest`, and ~370 tests run in CI across Linux, macOS, and Windows — with a dedicated MSRV check.

---

## Data & licensing

The Light only embeds texts that are **public domain or CC-BY**; copyrighted versions are reached exclusively through user-credentialed connectors (see [`DATA_SOURCES.md`](DATA_SOURCES.md)).

| Data | License | Notes |
|---|---|---|
| **King James Version (1769)** | Public domain | ~31,102 verses (EN) |
| **Almeida 1911** | Public domain | ~31,101 verses (PT) |
| **STEPBible** — TAHOT/TAGNT tokens, TBESH/TBESG lexicons | CC BY 4.0 | Original-language data + Strong's |
| **OpenBible.info** cross-references | CC-BY | ~344,799 references |

**Required attributions:**

> Cross references courtesy of [OpenBible.info](https://www.openbible.info/labs/cross-references/) (CC-BY).

> Credit it to 'STEP Bible' linked to [www.STEPBible.org](https://www.STEPBible.org) (data based on work at Tyndale House, Cambridge; CC BY 4.0).

---

## Project status

The Light is at **v1.2.0**. Shipped and working today:

- ✅ Reading & search (offline, bilingual, multi-version)
- ✅ Personal study — highlights, notes, cross-references, export
- ✅ Full ratatui TUI — clickable, mouse-selectable, themeable
- ✅ Reading plans with progress + `.ics` export
- ✅ Opt-in BYOK AI study — modes, lenses, depths, grounding, academic export
- ✅ Protected-version connectors (API.Bible, ESV)

*Planned:* a published Homebrew tap (`brew install butkeraites/tap/light`).

---

## Development

Requires stable Rust (install via [rustup](https://rustup.rs)).

```sh
cargo build                     # build the workspace
cargo test                      # run the test suite (~370 tests)
cargo clippy -- -D warnings     # lint (no warnings)
cargo fmt --check               # formatting
```

Project docs: [`SPEC.md`](SPEC.md) (vision & architecture) · [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) (roadmap) · [`DATA_SOURCES.md`](DATA_SOURCES.md) (provenance) · [`DECISIONS.md`](DECISIONS.md) (ADRs).

---

## License

Code is dual-licensed under **MIT OR Apache-2.0** — your choice. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE). Bundled Bible data follows its own licenses ([`DATA_SOURCES.md`](DATA_SOURCES.md)): only public-domain / free versions are embedded; protected versions are reached only via connectors with the user's own credentials.

---

## Acknowledgements

- **[STEP Bible](https://www.STEPBible.org)** & **Tyndale House, Cambridge** — original-language tokens and lexicons (CC BY 4.0).
- **[OpenBible.info](https://www.openbible.info/labs/cross-references/)** — the cross-reference dataset (CC-BY).
- The public-domain text projects (scrollmapper, damarals) that make the KJV and Almeida 1911 freely available.
- **[ratatui](https://ratatui.rs)** — the terminal UI framework behind the experience.

<div align="center">

*Built in Rust. Offline by default. Yours by design.* ✦

</div>
