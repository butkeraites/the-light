//! Camada de IA (opcional, *bring-your-own-key*).
//!
//! Abstrai um provedor de LLM ([`LlmProvider`]) por trás de uma interface de
//! *chat* simples; a orquestração de estudo/pergunta (prompt + RAG local) vive
//! aqui e é independente do provedor. As chaves ficam fora do git (ver
//! [`keys::KeyStore`]). A saída sempre separa **texto citado** (do banco local,
//! exato) de **interpretação** (do modelo) — ver `SPEC.md` §6.2.

pub mod keys;
pub mod prompts;
pub mod providers;
pub mod study;

pub use keys::KeyStore;
pub use providers::{build_provider, estimate_cost_usd};
pub use study::{
    ask, ask_context, ask_session, numbered_passage, numbered_verses, study, StudyRequest,
    StudyResult,
};

use std::str::FromStr;

/// Provedores de IA suportados (nomes válidos para `config set provider`).
pub const PROVIDERS: &[&str] = &["anthropic", "openai", "ollama"];

/// Lente denominacional aplicada ao estudo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denomination {
    /// Batista.
    Baptist,
    /// Presbiteriana / Reformada.
    Presbyterian,
    /// Luterana.
    Lutheran,
    /// Pentecostal.
    Pentecostal,
    /// Católica Romana.
    Catholic,
    /// Ortodoxa.
    Orthodox,
}

impl Denomination {
    /// Slug estável (usado em nomes de arquivo e prompts override).
    pub fn slug(self) -> &'static str {
        match self {
            Denomination::Baptist => "baptist",
            Denomination::Presbyterian => "presbyterian",
            Denomination::Lutheran => "lutheran",
            Denomination::Pentecostal => "pentecostal",
            Denomination::Catholic => "catholic",
            Denomination::Orthodox => "orthodox",
        }
    }

    /// Nome em português (para exibição).
    pub fn name_pt(self) -> &'static str {
        match self {
            Denomination::Baptist => "Batista",
            Denomination::Presbyterian => "Presbiteriana",
            Denomination::Lutheran => "Luterana",
            Denomination::Pentecostal => "Pentecostal",
            Denomination::Catholic => "Católica",
            Denomination::Orthodox => "Ortodoxa",
        }
    }

    /// Todas as lentes.
    pub fn all() -> [Denomination; 6] {
        [
            Denomination::Baptist,
            Denomination::Presbyterian,
            Denomination::Lutheran,
            Denomination::Pentecostal,
            Denomination::Catholic,
            Denomination::Orthodox,
        ]
    }
}

impl FromStr for Denomination {
    type Err = AiError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "baptist" | "batista" => Ok(Denomination::Baptist),
            "presbyterian" | "presbiteriana" | "reformada" | "reformed" => {
                Ok(Denomination::Presbyterian)
            }
            "lutheran" | "luterana" => Ok(Denomination::Lutheran),
            "pentecostal" => Ok(Denomination::Pentecostal),
            "catholic" | "católica" | "catolica" => Ok(Denomination::Catholic),
            "orthodox" | "ortodoxa" => Ok(Denomination::Orthodox),
            other => Err(AiError::UnknownLens(other.to_string())),
        }
    }
}

/// Profundidade do estudo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudyDepth {
    /// Visão geral.
    Overview,
    /// Exegético (contexto histórico-literário, estrutura).
    Exegetical,
    /// Estudo de palavras (grego/hebraico).
    WordStudy,
}

impl FromStr for StudyDepth {
    type Err = AiError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "overview" | "geral" | "visao" => Ok(StudyDepth::Overview),
            "exegetical" | "exegetico" | "exegético" => Ok(StudyDepth::Exegetical),
            "wordstudy" | "word-study" | "palavras" => Ok(StudyDepth::WordStudy),
            other => Err(AiError::UnknownDepth(other.to_string())),
        }
    }
}

impl StudyDepth {
    /// Nome em português.
    pub fn name_pt(self) -> &'static str {
        match self {
            StudyDepth::Overview => "visão geral",
            StudyDepth::Exegetical => "exegético",
            StudyDepth::WordStudy => "estudo de palavras",
        }
    }
}

/// Erros da camada de IA.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    /// Nenhuma chave configurada para o provedor.
    #[error("nenhuma chave para `{0}` — use `light config set-key {0} <chave>`")]
    NoKey(String),
    /// Nenhum provedor selecionado.
    #[error("nenhum provedor de IA selecionado — use `light config set provider <nome>`")]
    NoProvider,
    /// Provedor desconhecido.
    #[error("provedor desconhecido: `{0}` (use: anthropic, openai, ollama)")]
    UnknownProvider(String),
    /// Lente denominacional desconhecida.
    #[error("lente desconhecida: `{0}`")]
    UnknownLens(String),
    /// Profundidade desconhecida.
    #[error("profundidade desconhecida: `{0}`")]
    UnknownDepth(String),
    /// Erro de I/O.
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    /// Erro de rede/HTTP.
    #[error("erro de rede: {0}")]
    Http(String),
    /// Resposta inesperada do provedor.
    #[error("resposta inesperada do provedor: {0}")]
    BadResponse(String),
    /// TOML inválido (secrets).
    #[error("secrets inválido: {0}")]
    Toml(#[from] toml::de::Error),
    /// Erro ao serializar TOML.
    #[error("erro ao serializar secrets: {0}")]
    TomlSer(#[from] toml::ser::Error),
    /// Diretório de configuração indisponível.
    #[error("não foi possível determinar o diretório de configuração")]
    NoConfigDir,
}

/// Resultado da camada de IA.
pub type Result<T> = std::result::Result<T, AiError>;

/// Papel de uma mensagem numa conversa multi-turno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// Mensagem do usuário.
    User,
    /// Resposta do assistente.
    Assistant,
}

impl ChatRole {
    /// String do papel no formato das APIs (`"user"`/`"assistant"`).
    pub fn as_str(self) -> &'static str {
        match self {
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
        }
    }
}

/// Uma mensagem de uma conversa (turno) enviada ao provedor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// Papel (usuário/assistente).
    pub role: ChatRole,
    /// Conteúdo textual.
    pub content: String,
}

/// Interface de *chat* de um provedor de LLM.
pub trait LlmProvider {
    /// Nome do provedor (ex.: `"anthropic"`).
    fn name(&self) -> &str;

    /// Modelo configurado.
    fn model(&self) -> &str;

    /// Envia `system` + `user` e devolve a resposta de texto.
    fn complete(&self, system: &str, user: &str) -> Result<String>;

    /// Conversa multi-turno: `system` + histórico de mensagens. A implementação
    /// padrão dobra o histórico num transcript e chama [`LlmProvider::complete`]
    /// (suficiente para `mock` e provedores que não suportam multi-mensagem);
    /// os provedores reais sobrescrevem com um corpo de mensagens nativo.
    fn chat(&self, system: &str, messages: &[ChatMessage]) -> Result<String> {
        let mut user = String::new();
        for m in messages {
            let who = match m.role {
                ChatRole::User => "Usuário",
                ChatRole::Assistant => "Assistente",
            };
            user.push_str(&format!("{who}: {}\n\n", m.content));
        }
        self.complete(system, user.trim_end())
    }

    /// Estimativa grosseira de tokens (≈ 4 caracteres por token).
    fn estimate_tokens(&self, text: &str) -> usize {
        text.chars().count().div_ceil(4)
    }
}

/// Provedor falso para testes/demos: devolve uma resposta fixa (sem rede).
pub struct MockLlmProvider {
    response: String,
    model: String,
}

impl MockLlmProvider {
    /// Mock com uma resposta canônica.
    pub fn new(response: impl Into<String>) -> Self {
        MockLlmProvider {
            response: response.into(),
            model: "mock-1".to_string(),
        }
    }
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        MockLlmProvider::new(
            "Interpretação simulada (provedor de teste). \
             A passagem é citada acima a partir do texto local.",
        )
    }
}

impl LlmProvider for MockLlmProvider {
    fn name(&self) -> &str {
        "mock"
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn complete(&self, _system: &str, _user: &str) -> Result<String> {
        Ok(self.response.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denomination_parsing_pt_en() {
        assert_eq!(
            "batista".parse::<Denomination>().unwrap(),
            Denomination::Baptist
        );
        assert_eq!(
            "presbiteriana".parse::<Denomination>().unwrap(),
            Denomination::Presbyterian
        );
        assert_eq!(
            "Reformed".parse::<Denomination>().unwrap(),
            Denomination::Presbyterian
        );
        assert_eq!(
            "católica".parse::<Denomination>().unwrap(),
            Denomination::Catholic
        );
        assert!("jedi".parse::<Denomination>().is_err());
        assert_eq!(Denomination::Lutheran.slug(), "lutheran");
        assert_eq!(Denomination::all().len(), 6);
    }

    #[test]
    fn depth_parsing() {
        assert_eq!(
            "exegetical".parse::<StudyDepth>().unwrap(),
            StudyDepth::Exegetical
        );
        assert_eq!(
            "palavras".parse::<StudyDepth>().unwrap(),
            StudyDepth::WordStudy
        );
        assert!("deep".parse::<StudyDepth>().is_err());
    }

    #[test]
    fn mock_provider_completes() {
        let m = MockLlmProvider::new("ola");
        assert_eq!(m.name(), "mock");
        assert_eq!(m.complete("sys", "user").unwrap(), "ola");
        assert!(m.estimate_tokens("abcdefgh") >= 2);
    }
}
