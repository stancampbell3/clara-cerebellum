//! Blocking HTTP clients for the live topology view: ritual summaries from
//! Dis (clara-api, unauthenticated — same "trusted caller on the LAN"
//! posture as every other Dis endpoint) and this FieryPit's own evaluator
//! liveness from lildaemon's `GET /ritual/participants`.
//!
//! Rituals and evaluators are joined purely on `ritual_id`, the one
//! identifier both sides agree on — Dis's `participants` map is keyed by a
//! caller-supplied `participant_key` (typically a FieryPit URL), which is a
//! different identity space from lildaemon's own per-node `node_id`, so no
//! attempt is made to cross-reference those two.

use reqwest::blocking::Client;
use serde::Deserialize;

use crate::assistant_client::AssistantError;

#[derive(Debug, Clone, Deserialize)]
pub struct RitualSummary {
    pub ritual_id: String,
    pub name: String,
    pub state: String,
    #[allow(dead_code)]
    pub topic: String,
}

#[derive(Debug, Deserialize)]
struct RitualListResponse {
    rituals: Vec<RitualSummary>,
}

/// GET {dis_base_url}/ritual — every currently active Ritual.
pub fn list_rituals(http: &Client, dis_base_url: &str) -> Result<Vec<RitualSummary>, AssistantError> {
    let resp = http.get(format!("{dis_base_url}/ritual")).send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "list_rituals failed ({status}): {body}"
        )));
    }
    Ok(resp.json::<RitualListResponse>()?.rituals)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParticipantInfo {
    pub ritual_id: String,
    pub node_id: String,
    pub evaluator_name: Option<String>,
    /// starting/idle/busy/error/stopped — goat/models/RitualParticipant.ParticipantState.
    pub state: String,
}

#[derive(Debug, Deserialize)]
struct ParticipantsResponse {
    participants: Vec<ParticipantInfo>,
}

/// GET {fiery_pit_url}/ritual/participants — every RitualParticipant this
/// FieryPit currently holds, across all rituals, with its live state.
pub fn list_participants(
    http: &Client,
    fiery_pit_url: &str,
    token: &str,
) -> Result<Vec<ParticipantInfo>, AssistantError> {
    let resp = http
        .get(format!("{fiery_pit_url}/ritual/participants"))
        .bearer_auth(token)
        .send()?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(AssistantError::Api(format!(
            "list_participants failed ({status}): {body}"
        )));
    }
    Ok(resp.json::<ParticipantsResponse>()?.participants)
}
