//! Resolução de fonte por slug: versão local (embarcada) primeiro; senão, um
//! conector configurado (versão protegida, lida ao vivo com a chave do cofre).

use the_light_core::ai::KeyStore;
use the_light_core::config::Config;
use the_light_core::model::{License, Translation, TranslationId};
use the_light_core::source::{ApiBibleSource, BibleSource, EmbeddedSource, EsvApiSource};
use the_light_core::store::Store;

/// Resolve a fonte para `slug`. `Err` traz uma mensagem amigável (sem rede).
pub fn resolve<'a>(
    store: &'a Store,
    config: &Config,
    slug: &str,
) -> Result<Box<dyn BibleSource + 'a>, String> {
    let tid = TranslationId::new(slug);

    let embedded = EmbeddedSource::new(store);
    if embedded.has_translation(&tid).unwrap_or(false) {
        return Ok(Box::new(embedded));
    }

    let Some(c) = config
        .connectors
        .iter()
        .find(|c| TranslationId::new(&c.slug) == tid)
    else {
        return Err(format!(
            "versão desconhecida: `{slug}` (não está no banco nem configurada como conector)"
        ));
    };

    let translation = Translation {
        id: tid.clone(),
        abbrev: c.abbrev.clone(),
        name: c.name.clone(),
        language: c.language,
        license: License::Proprietary,
        embeddable: false,
    };
    // Chave por tipo de conector (apibible/esv), nunca no config.toml.
    let key = KeyStore::open_default()
        .ok()
        .and_then(|ks| ks.get(&c.kind).map(str::to_string));

    match c.kind.as_str() {
        "apibible" => {
            let key = key.ok_or_else(|| {
                "conector API.Bible sem chave — use `light config set-key apibible <chave>`"
                    .to_string()
            })?;
            let bible_id = c.bible_id.clone().ok_or_else(|| {
                format!(
                    "conector `{}` sem `bible_id` (reconfigure com --bible-id)",
                    c.slug
                )
            })?;
            Ok(Box::new(ApiBibleSource::new(bible_id, translation, key)))
        }
        "esv" => {
            let key = key.ok_or_else(|| {
                "conector ESV sem chave — use `light config set-key esv <chave>`".to_string()
            })?;
            Ok(Box::new(EsvApiSource::new(translation, key)))
        }
        other => Err(format!(
            "tipo de conector desconhecido: `{other}` (use apibible ou esv)"
        )),
    }
}
