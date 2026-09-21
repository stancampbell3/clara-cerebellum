//! Blocking HTTP client for lildaemon's goat/app/assistant/ REST API.
//!
//! Replaces the old deduce.rs (direct clara-api /deduce polling) and the
//! FieryPitClient-based /evaluate call — the assistant backend now owns the
//! whole per-turn deduction/LLM orchestration, so this frontend only needs
//! auth + three simple REST calls.

use reqwest::blocking::{multipart, Client};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssistantError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("assistant API error: {0}")]
    Api(String),
}

/// POST /auth/login-demo — no-password, per-visitor login. Upserts a
/// service-role account by username and returns a bearer JWT scoped to
/// that one identity (replaces the old shared-service-account
/// register_and_login, which every browser session used to reuse).
pub fn login_demo(http: &Client, base_url: &str, username: &str) -> Result<String, AssistantError> {
    let resp = http
        .post(format!("{base_url}/auth/login-demo"))
        .json(&json!({"username": username}))
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "login-demo failed ({status}): {body}"
        )));
    }
    let body: Value = resp.json()?;
    body["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| AssistantError::Api(format!("no access_token in response: {body}")))
}

/// POST /assistant/sessions — returns the new session_id.
pub fn create_session(http: &Client, base_url: &str, token: &str) -> Result<String, AssistantError> {
    let resp = http
        .post(format!("{base_url}/assistant/sessions"))
        .bearer_auth(token)
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "create_session failed ({status}): {body}"
        )));
    }
    let body: Value = resp.json()?;
    body["session_id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| AssistantError::Api(format!("no session_id in response: {body}")))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RulesetInfo {
    pub ruleset_key: String,
    pub label: String,
    pub description: String,
}

/// GET /assistant/rulesets — available rulesets for the ruleset dropdown.
pub fn list_rulesets(http: &Client, base_url: &str, token: &str) -> Result<Vec<RulesetInfo>, AssistantError> {
    let resp = http
        .get(format!("{base_url}/assistant/rulesets"))
        .bearer_auth(token)
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "list_rulesets failed ({status}): {body}"
        )));
    }
    Ok(resp.json()?)
}

/// PUT /assistant/sessions/{id}/ruleset — set this session's active ruleset.
pub fn set_session_ruleset(
    http: &Client,
    base_url: &str,
    token: &str,
    session_id: &str,
    ruleset_key: &str,
) -> Result<(), AssistantError> {
    let resp = http
        .put(format!("{base_url}/assistant/sessions/{session_id}/ruleset"))
        .bearer_auth(token)
        .json(&json!({"ruleset_key": ruleset_key}))
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "set_session_ruleset failed ({status}): {body}"
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct SendResponse {
    pub reply: String,
    pub action_taken: String,
    pub workspace_slug: Option<String>,
    #[serde(default)]
    pub citation_count: u32,
}

/// POST /assistant/sessions/{id}/send — the core per-turn call.
pub fn send(
    http: &Client,
    base_url: &str,
    token: &str,
    session_id: &str,
    text: &str,
) -> Result<SendResponse, AssistantError> {
    let resp = http
        .post(format!("{base_url}/assistant/sessions/{session_id}/send"))
        .bearer_auth(token)
        .json(&json!({"text": text}))
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "send failed ({status}): {body}"
        )));
    }
    Ok(resp.json()?)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AttachmentResponse {
    pub filename: String,
    pub size: u64,
    pub content_type: Option<String>,
    pub inlined: bool,
}

/// POST /assistant/sessions/{id}/attachments — upload a chat file, dropped
/// into the browser, into this session's analyst workspace (see lildaemon's
/// goat/app/assistant/runtime.py::save_attachment). Validates `content_type`
/// as a real MIME string on a throwaway empty Part first, since
/// `Part::mime_str` consumes the Part it's called on — this lets us fall
/// back to no explicit Content-Type on the real (already-moved-in) `bytes`
/// without losing them if the browser ever sends something malformed.
pub fn upload_attachment(
    http: &Client,
    base_url: &str,
    token: &str,
    session_id: &str,
    filename: &str,
    content_type: Option<&str>,
    bytes: Vec<u8>,
) -> Result<AttachmentResponse, AssistantError> {
    let valid_mime = content_type
        .map(|ct| multipart::Part::bytes(Vec::new()).mime_str(ct).is_ok())
        .unwrap_or(false);

    let mut part = multipart::Part::bytes(bytes).file_name(filename.to_string());
    if valid_mime {
        part = part
            .mime_str(content_type.expect("checked above"))
            .expect("validated above");
    }
    let form = multipart::Form::new().part("file", part);

    let resp = http
        .post(format!(
            "{base_url}/assistant/sessions/{session_id}/attachments"
        ))
        .bearer_auth(token)
        .multipart(form)
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "upload_attachment failed ({status}): {body}"
        )));
    }
    Ok(resp.json()?)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PendingResearchInfo {
    pub request_id: String,
    pub query: String,
    pub reply: Option<String>,
    #[serde(default)]
    pub citation_count: u32,
    /// "research" (deferred_query) | "deliberation" | "brainstorm" — was
    /// silently dropped here before 2026-09-13 (the backend already
    /// returned it), leaving the client with no way to style/label
    /// different background-work kinds differently.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// "researching" | "answering" | "ready" | "delivered" | "failed" —
    /// new 2026-09-13 (id_ritual_of_rituals_planning.md Part 2): the GET
    /// endpoint now returns non-terminal rows too (a safe, repeatable
    /// peek — see router.py's list_pending_research docstring), so the
    /// client can render an in-progress indicator, not just delivered
    /// results.
    pub status: String,
    /// kind="escalation": "<ritual_id>/<seq>" of the action awaiting approve/deny. Opaque to the client.
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
}

fn default_kind() -> String {
    "research".to_string()
}

/// GET /assistant/sessions/{id}/pending-research — every outstanding
/// background turn for this session (researching/answering/ready),
/// WITHOUT marking anything delivered. Safe to call repeatedly — see
/// ack_pending_research below for the step that actually retires a
/// 'ready' row. Renamed in spirit (not in name, to keep this a minimal
/// diff) from a destructive drain to a non-destructive peek 2026-09-13 —
/// see router.py's list_pending_research docstring for the full
/// rationale (a real delivery-loss window this fixes).
pub fn list_pending_research(
    http: &Client,
    base_url: &str,
    token: &str,
    session_id: &str,
) -> Result<Vec<PendingResearchInfo>, AssistantError> {
    let resp = http
        .get(format!("{base_url}/assistant/sessions/{session_id}/pending-research"))
        .bearer_auth(token)
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "list_pending_research failed ({status}): {body}"
        )));
    }
    Ok(resp.json()?)
}

/// POST /assistant/sessions/{id}/pending-research/{request_id}/ack —
/// acknowledge one 'ready' row as delivered. Called only once the client
/// has durably persisted the result (see ws.rs's ResearchAck handling) —
/// this is the step that closes the delivery-loss window a plain GET used
/// to leave open by marking delivered on read instead of on ack.
pub fn ack_pending_research(
    http: &Client,
    base_url: &str,
    token: &str,
    session_id: &str,
    request_id: &str,
) -> Result<(), AssistantError> {
    let resp = http
        .post(format!(
            "{base_url}/assistant/sessions/{session_id}/pending-research/{request_id}/ack"
        ))
        .bearer_auth(token)
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "ack_pending_research failed ({status}): {body}"
        )));
    }
    Ok(())
}

/// The user's answer to an escalated Ego action, as the backend reports it.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct EscalationOutcome {
    /// "approved" | "denied"
    pub state: String,
    #[serde(default)]
    pub executed: bool,
    #[serde(default)]
    pub outcome: Option<String>,
    /// Ready-to-show sentence: what was (or was not) done.
    pub message: String,
}

/// Turn a resolve response into an outcome, or into an error carrying the backend's own `detail`
/// (404 gone, 409 already handled, 410 expired, ...). Pure, so it is unit-tested without a server.
pub fn parse_resolve_response(status: u16, body: &str) -> Result<EscalationOutcome, AssistantError> {
    if (200..300).contains(&status) {
        return serde_json::from_str(body)
            .map_err(|e| AssistantError::Api(format!("bad resolve response: {e}")));
    }
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("detail").and_then(|d| d.as_str().map(str::to_string)))
        .unwrap_or_else(|| body.to_string());
    Err(AssistantError::Api(format!("resolve_escalation failed ({status}): {detail}")))
}

/// POST /assistant/sessions/{id}/escalations/{request_id}/resolve — the logged-in user's approve/deny of an
/// action the Ego's Superego escalated. The backend checks the session belongs to this token's user.
pub fn resolve_escalation(
    http: &Client,
    base_url: &str,
    token: &str,
    session_id: &str,
    request_id: &str,
    decision: &str,
) -> Result<EscalationOutcome, AssistantError> {
    let resp = http
        .post(format!(
            "{base_url}/assistant/sessions/{session_id}/escalations/{request_id}/resolve"
        ))
        .bearer_auth(token)
        .json(&json!({ "decision": decision }))
        .send()?;
    let status = resp.status().as_u16();
    let body = resp.text().unwrap_or_default();
    parse_resolve_response(status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_approved_outcome() {
        let out = parse_resolve_response(
            200,
            r#"{"state":"approved","executed":true,"outcome":"published 'plan.md'","message":"Approved. published 'plan.md'"}"#,
        )
        .unwrap();
        assert_eq!(out.state, "approved");
        assert!(out.executed);
        assert_eq!(out.message, "Approved. published 'plan.md'");
    }

    #[test]
    fn parses_a_denial_without_optional_fields() {
        let out = parse_resolve_response(200, r#"{"state":"denied","message":"Denied."}"#).unwrap();
        assert!(!out.executed);
        assert_eq!(out.outcome, None);
    }

    #[test]
    fn carries_the_backends_detail_on_errors() {
        for (status, detail) in [(404, "no such escalation"), (409, "this escalation was already handled"), (410, "expired")] {
            let body = format!(r#"{{"detail":"{detail}"}}"#);
            let err = parse_resolve_response(status, &body).unwrap_err().to_string();
            assert!(err.contains(&status.to_string()) && err.contains(detail), "{err}");
        }
    }

    #[test]
    fn tolerates_a_non_json_error_body_and_a_garbled_success() {
        assert!(parse_resolve_response(502, "Bad Gateway").unwrap_err().to_string().contains("Bad Gateway"));
        assert!(parse_resolve_response(200, "not json").is_err());
    }

    #[test]
    fn the_pending_entry_accepts_the_new_ref_field_and_its_absence() {
        // The backend field is `ref`; serde maps it via the rename on the struct field.
        let with_ref: PendingResearchInfo = serde_json::from_str(
            r#"{"request_id":"r","query":"q","reply":"t","kind":"escalation","status":"ready","ref":"ritual/3"}"#,
        )
        .unwrap();
        assert_eq!(with_ref.reference.as_deref(), Some("ritual/3"));
        let without: PendingResearchInfo =
            serde_json::from_str(r#"{"request_id":"r","query":"q","reply":null,"status":"researching"}"#).unwrap();
        assert_eq!(without.reference, None);
        assert_eq!(without.kind, "research");
    }
}
