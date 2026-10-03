//! Assistant de la palette : Claude (Anthropic) ou Gemini (Google), au choix,
//! avec la clé API de l'utilisateur. La réponse arrive morceau par morceau.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Deserialize, Clone)]
pub struct ChatMessage {
    /// « user » ou « assistant »
    pub role: String,
    pub content: String,
}

/// La réponse s'affiche dans une petite fenêtre : on demande du court.
const SYSTEM: &str = "Tu es l'assistant intégré à Kiosky, une boîte à outils pour Windows. \
Tes réponses s'affichent dans une petite fenêtre de recherche : va droit au but, en quelques phrases, \
et ne développe que si on te le demande. Réponds dans la langue de la question. \
Tu peux utiliser du Markdown simple (gras, listes, blocs de code). \
Quand on te demande de corriger, traduire ou reformuler un texte, renvoie uniquement le texte obtenu, \
sans introduction ni commentaire, pour qu'il puisse être copié tel quel.";

// ───────────────────────────── Clés API ─────────────────────────────
//
// Les clés ne sont pas dans settings.json (lu par toutes les fenêtres) : elles sont chiffrées par
// Windows pour le compte de l'utilisateur (DPAPI) dans un fichier à part, et l'interface ne les relit jamais.

#[derive(Serialize, Deserialize, Default)]
struct Keys {
    claude: String,
    gemini: String,
}

#[derive(Serialize)]
pub struct KeyStatus {
    pub claude: bool,
    pub gemini: bool,
}

fn keys_path(dir: &Path) -> PathBuf {
    dir.join("ai-keys.json")
}

fn load_keys(dir: &Path) -> Keys {
    std::fs::read_to_string(keys_path(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn dpapi(data: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        let result = if protect {
            CryptProtectData(&input, None, None, None, None, 0, &mut output)
        } else {
            CryptUnprotectData(&input, None, None, None, None, 0, &mut output)
        };
        result.map_err(|e| format!("Chiffrement de la clé impossible : {e}"))?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(output.pbData.cast()));
        Ok(bytes)
    }
}

/// Enregistre la clé d'un fournisseur (« claude » ou « gemini »). Une clé vide la supprime.
pub fn set_key(dir: &Path, provider: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    let stored = if key.is_empty() {
        String::new()
    } else {
        base64::engine::general_purpose::STANDARD.encode(dpapi(key.as_bytes(), true)?)
    };
    let mut keys = load_keys(dir);
    match provider {
        "claude" => keys.claude = stored,
        "gemini" => keys.gemini = stored,
        _ => return Err(format!("Fournisseur inconnu : {provider}")),
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&keys).map_err(|e| e.to_string())?;
    std::fs::write(keys_path(dir), json).map_err(|e| e.to_string())
}

pub fn key_status(dir: &Path) -> KeyStatus {
    let keys = load_keys(dir);
    KeyStatus { claude: !keys.claude.is_empty(), gemini: !keys.gemini.is_empty() }
}

fn key(dir: &Path, provider: &str) -> Result<String, String> {
    let keys = load_keys(dir);
    let (stored, name) = if provider == "gemini" { (keys.gemini, "Gemini") } else { (keys.claude, "Claude") };
    if stored.is_empty() {
        return Err(format!("Aucune clé API pour {name}. Ajoute-la dans Réglages → Assistant."));
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(stored).map_err(|e| e.to_string())?;
    String::from_utf8(dpapi(&bytes, false)?).map_err(|e| e.to_string())
}

// ───────────────────────────── Requêtes ─────────────────────────────

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            // Pas de durée totale : une longue réponse reste ouverte tant qu'elle avance.
            .read_timeout(Duration::from_secs(120))
            .user_agent(concat!("Kiosky/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("client HTTP")
    })
}

/// Identifiant de la réponse en cours. Une nouvelle question ou « Arrêter » le change,
/// et la lecture de l'ancienne réponse s'interrompt.
static CURRENT: AtomicU64 = AtomicU64::new(0);

pub fn stop() {
    CURRENT.store(0, Ordering::SeqCst);
}

/// Ce qu'une ligne « data: » du flux apporte.
#[derive(Debug, PartialEq)]
enum Event {
    Text(String),
    /// Fin anormale, à montrer à l'utilisateur
    Error(String),
    None,
}

fn claude_event(v: &Value) -> Event {
    match v["type"].as_str() {
        Some("content_block_delta") if v["delta"]["type"] == "text_delta" => {
            Event::Text(v["delta"]["text"].as_str().unwrap_or_default().to_string())
        }
        Some("message_delta") => match v["delta"]["stop_reason"].as_str() {
            Some("refusal") => Event::Error("Le modèle a refusé de répondre à cette demande.".into()),
            Some("max_tokens") => Event::Error("Réponse coupée : elle dépassait la longueur maximale.".into()),
            _ => Event::None,
        },
        Some("error") => Event::Error(v["error"]["message"].as_str().unwrap_or("Erreur du service").to_string()),
        _ => Event::None,
    }
}

fn gemini_event(v: &Value) -> Event {
    if let Some(msg) = v["error"]["message"].as_str() {
        return Event::Error(msg.to_string());
    }
    if let Some(reason) = v["promptFeedback"]["blockReason"].as_str() {
        return Event::Error(format!("Gemini a bloqué cette demande ({reason})."));
    }
    let candidate = &v["candidates"][0];
    let text: String = candidate["content"]["parts"]
        .as_array()
        .map(|parts| {
            // Les morceaux marqués « thought » sont le raisonnement du modèle, pas la réponse.
            parts.iter().filter(|p| p["thought"] != true).filter_map(|p| p["text"].as_str()).collect()
        })
        .unwrap_or_default();
    if !text.is_empty() {
        return Event::Text(text);
    }
    match candidate["finishReason"].as_str() {
        Some("STOP") | None => Event::None,
        Some("MAX_TOKENS") => Event::Error("Réponse coupée : elle dépassait la longueur maximale.".into()),
        Some(reason) => Event::Error(format!("Gemini a interrompu la réponse ({reason}).")),
    }
}

/// Message lisible pour une réponse HTTP en erreur.
fn http_error(status: u16, body: &str, provider: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| body.chars().take(200).collect());
    match status {
        401 | 403 => format!("Clé API refusée par {provider}. Vérifie-la dans Réglages → Assistant. ({detail})"),
        404 => format!("Modèle introuvable chez {provider}. Vérifie-le dans Réglages → Assistant. ({detail})"),
        429 => format!("Limite d'utilisation atteinte chez {provider}. ({detail})"),
        _ => format!("{provider} a répondu par une erreur {status}. ({detail})"),
    }
}

/// Modèles qui acceptent le repli automatique quand une demande est déclinée.
const CLAUDE_FALLBACK: &[&str] = &["claude-fable-5-1", "claude-opus-5-5", "claude-opus-5", "claude-sonnet-5-5"];

fn claude_request(key: &str, model: &str, messages: &[ChatMessage]) -> reqwest::RequestBuilder {
    let mut body = json!({
        "model": model,
        "max_tokens": 16000,
        "stream": true,
        "system": SYSTEM,
        "messages": messages.iter().map(|m| json!({ "role": m.role, "content": m.content })).collect::<Vec<_>>(),
    });
    let mut request = http()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01");
    // Haiku 4.5 ne connaît pas le réglage d'effort. Ailleurs, « low » : des réponses rapides et courtes.
    if !model.contains("haiku") {
        body["output_config"] = json!({ "effort": "low" });
    }
    if CLAUDE_FALLBACK.contains(&model) {
        body["fallbacks"] = json!("default");
        request = request.header("anthropic-beta", "server-side-fallback-2026-07-01");
    }
    request.json(&body)
}

fn gemini_request(key: &str, model: &str, messages: &[ChatMessage]) -> Result<reqwest::RequestBuilder, String> {
    if model.is_empty() || !model.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c)) {
        return Err(format!("Nom de modèle invalide : « {model} »"));
    }
    let contents: Vec<Value> = messages
        .iter()
        .map(|m| json!({ "role": if m.role == "assistant" { "model" } else { "user" }, "parts": [{ "text": m.content }] }))
        .collect();
    let body = json!({ "system_instruction": { "parts": [{ "text": SYSTEM }] }, "contents": contents });
    Ok(http()
        .post(format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse"))
        .header("x-goog-api-key", key)
        .json(&body))
}

/// Envoie la conversation et appelle `on_text` à chaque morceau de réponse.
pub async fn chat(
    dir: &Path,
    provider: &str,
    model: &str,
    messages: &[ChatMessage],
    id: u64,
    mut on_text: impl FnMut(&str),
) -> Result<(), String> {
    let gemini = provider == "gemini";
    let name = if gemini { "Gemini" } else { "Claude" };
    let key = key(dir, provider)?;
    let request = if gemini { gemini_request(&key, model, messages)? } else { claude_request(&key, model, messages) };
    CURRENT.store(id, Ordering::SeqCst);

    let mut response = request.send().await.map_err(|e| format!("Impossible de joindre {name} : {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(http_error(status.as_u16(), &body, name));
    }

    // Flux d'événements : des lignes « data: {json} ». Un morceau réseau peut couper une ligne
    // (et même un caractère) en deux : on ne traite que les lignes complètes.
    let mut buffer: Vec<u8> = Vec::new();
    loop {
        if CURRENT.load(Ordering::SeqCst) != id {
            return Ok(()); // arrêté, ou remplacé par une nouvelle question
        }
        let Some(chunk) = response.chunk().await.map_err(|e| format!("Connexion à {name} interrompue : {e}"))? else {
            return Ok(());
        };
        buffer.extend_from_slice(&chunk);
        while let Some(end) = buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(data) = line.trim().strip_prefix("data:") else { continue };
            let Ok(value) = serde_json::from_str::<Value>(data.trim()) else { continue };
            match if gemini { gemini_event(&value) } else { claude_event(&value) } {
                Event::Text(text) => on_text(&text),
                Event::Error(message) => return Err(message),
                Event::None => {}
            }
        }
    }
}

/// Modèles Gemini utilisables avec cette clé, pour la liste des réglages.
pub async fn gemini_models(dir: &Path) -> Result<Vec<String>, String> {
    let key = key(dir, "gemini")?;
    let response = http()
        .get("https://generativelanguage.googleapis.com/v1beta/models?pageSize=200")
        .header("x-goog-api-key", key)
        .send()
        .await
        .map_err(|e| format!("Impossible de joindre Gemini : {e}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(http_error(status.as_u16(), &body, "Gemini"));
    }
    let value: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let mut models: Vec<String> = value["models"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter(|m| {
                    m["supportedGenerationMethods"].as_array().is_some_and(|a| a.iter().any(|x| x == "generateContent"))
                })
                .filter_map(|m| m["name"].as_str())
                .map(|n| n.trim_start_matches("models/").to_string())
                .filter(|n| n.starts_with("gemini"))
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_stream_events() {
        let text = json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Bonjour"}});
        assert_eq!(claude_event(&text), Event::Text("Bonjour".into()));
        // Le raisonnement du modèle n'est pas affiché.
        let thinking = json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}});
        assert_eq!(claude_event(&thinking), Event::None);
        let end = json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}});
        assert_eq!(claude_event(&end), Event::None);
        let refusal = json!({"type":"message_delta","delta":{"stop_reason":"refusal"}});
        assert!(matches!(claude_event(&refusal), Event::Error(_)));
        let error = json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}});
        assert_eq!(claude_event(&error), Event::Error("Overloaded".into()));
    }

    #[test]
    fn gemini_stream_events() {
        let text = json!({"candidates":[{"content":{"role":"model","parts":[{"text":"Un commit "},{"text":"est…"}]}}]});
        assert_eq!(gemini_event(&text), Event::Text("Un commit est…".into()));
        let thought = json!({"candidates":[{"content":{"parts":[{"text":"réflexion","thought":true}]}}]});
        assert_eq!(gemini_event(&thought), Event::None);
        let end = json!({"candidates":[{"content":{"parts":[{"text":""}]},"finishReason":"STOP"}]});
        assert_eq!(gemini_event(&end), Event::None);
        let blocked = json!({"promptFeedback":{"blockReason":"SAFETY"}});
        assert!(matches!(gemini_event(&blocked), Event::Error(_)));
    }

    #[test]
    fn readable_http_errors() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        let msg = http_error(401, body, "Claude");
        assert!(msg.contains("Clé API refusée") && msg.contains("invalid x-api-key"));
    }

    #[test]
    fn key_round_trip() {
        let dir = std::env::temp_dir().join(format!("kiosky-ai-test-{}", std::process::id()));
        set_key(&dir, "gemini", "  clé-secrète-123 ").unwrap();
        // Chiffrée sur le disque, relue à l'identique.
        let raw = std::fs::read_to_string(keys_path(&dir)).unwrap();
        assert!(!raw.contains("secrète"));
        assert_eq!(key(&dir, "gemini").unwrap(), "clé-secrète-123");
        assert!(key(&dir, "claude").is_err());
        let status = key_status(&dir);
        assert!(status.gemini && !status.claude);
        set_key(&dir, "gemini", "").unwrap();
        assert!(!key_status(&dir).gemini);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
