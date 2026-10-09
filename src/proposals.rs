//! Human/agent proposals are immutable suggestions until explicitly resolved.
//! All numerical/review changes still execute through the shared engine.
use crate::{
    domain::{EngineError, Operation, Request, Result},
    engine::{Output, Response},
    jobs::JobControl,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub actor: String,
    pub reason: String,
    pub decision: String,
    pub unix_ms: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub version: u32,
    pub id: uuid::Uuid,
    pub request: Request,
    pub expected_project_revision: Option<u64>,
    pub before: Value,
    pub before_sha256: String,
    pub proposed: Value,
    pub reason: String,
    pub evidence: Vec<String>,
    pub events: Vec<Event>,
    pub applied: Option<Value>,
}
fn invalid(message: impl ToString) -> EngineError {
    EngineError::new("invalid_parameters", message)
}
pub fn same_subject(value: &Value, before: &Value) -> bool {
    for key in ["batch_id", "report_id", "id"] {
        if before[key].is_string() {
            return value[key] == before[key];
        }
    }
    if before["raw"]["id"].is_string() {
        return value["raw"]["id"] == before["raw"]["id"];
    }
    if before["matrix"].is_object() && before["hypotheses"].is_array() {
        return value["matrix"] == before["matrix"] && value["hypotheses"].is_array();
    }
    false
}
pub fn find_current(value: &Value, before: &Value) -> Option<Value> {
    let mut found = vec![];
    fn visit(value: &Value, before: &Value, found: &mut Vec<Value>) {
        if value.is_object() && same_subject(value, before) {
            found.push(value.clone());
            return;
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    visit(value, before, found)
                }
            }
            Value::Object(map) => {
                for value in map.values() {
                    visit(value, before, found)
                }
            }
            _ => {}
        }
    }
    visit(value, before, &mut found);
    found.into_iter().max_by_key(|value| {
        ["reviews", "revisions", "annotations"]
            .iter()
            .map(|key| value[*key].as_array().map_or(0, Vec::len))
            .sum::<usize>()
    })
}
pub fn before(operation: &Operation) -> Result<Value> {
    let value = match operation {
        Operation::ReviseChromatogram { analysis, .. } => serde_json::to_value(analysis),
        Operation::ReviewTargeted { batch, .. } => serde_json::to_value(batch),
        Operation::ReviewQc { report, .. } => serde_json::to_value(report),
        Operation::AnnotateSpectrum { report, .. } => serde_json::to_value(report),
        Operation::ReviewFeatureAnnotation { ledger, .. } => serde_json::to_value(ledger),
        _ => {
            return Err(invalid(
                "A review proposal must use a supported revision-bound review operation",
            ))
        }
    };
    value.map_err(invalid)
}
fn hash(value: &Value) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(invalid)?)
    ))
}
pub fn reviewed_value(output: &Output) -> Result<Value> {
    match output {
        Output::TargetedQuantification { batch } => serde_json::to_value(batch),
        Output::QcReport { report } => serde_json::to_value(report),
        Output::SpectralSearch { report } => serde_json::to_value(report),
        Output::FeatureAnnotations { ledger } => serde_json::to_value(ledger),
        Output::ChromatographicProcessing { analyses, .. } => serde_json::to_value(
            analyses
                .first()
                .ok_or_else(|| invalid("Review output is empty"))?,
        ),
        _ => return Err(invalid("Not a supported reviewed result")),
    }
    .map_err(invalid)
}
fn semantic_review(value: &Value, original: &Value) -> Value {
    let mut value = value.clone();
    for key in ["reviews", "revisions", "annotations"] {
        let old = original[key].as_array().map_or(0, Vec::len);
        if let Some(events) = value[key].as_array_mut() {
            for event in events.iter_mut().skip(old) {
                if let Some(event) = event.as_object_mut() {
                    for field in ["id", "unix_ms", "actor"] {
                        event.remove(field);
                    }
                }
            }
        }
    }
    value
}
pub fn prepare(
    path: &std::path::Path,
    request: Request,
    revision: Option<u64>,
    reason: String,
    evidence: Vec<String>,
    control: &JobControl,
) -> Result<Proposal> {
    if reason.trim().is_empty()
        || evidence.is_empty()
        || evidence.iter().any(|x| x.trim().is_empty())
    {
        return Err(invalid(
            "Proposal reason and evidence references are required",
        ));
    }
    if matches!(
        request.operation,
        Operation::ReviseChromatogram { preview: true, .. }
    ) {
        return Err(invalid(
            "A proposal must request a committed review; preparation itself does not apply it",
        ));
    }
    let original = before(&request.operation)?;
    let response = crate::engine::execute(path, request.clone(), control)?;
    Ok(Proposal {
        version: 1,
        id: uuid::Uuid::new_v4(),
        request,
        expected_project_revision: revision,
        before_sha256: hash(&original)?,
        before: original,
        proposed: reviewed_value(&response.output)?,
        reason,
        evidence,
        events: vec![],
        applied: None,
    })
}
#[expect(
    clippy::too_many_arguments,
    reason = "Preserve the explicit revision-bound review API: source, evidence, revision, attributed decision and cancellation"
)]
pub fn resolve(
    path: &std::path::Path,
    proposal: &Proposal,
    current: &Value,
    revision: Option<u64>,
    approve: bool,
    actor: &str,
    reason: &str,
    control: &JobControl,
) -> Result<(Proposal, Option<Response>)> {
    if proposal.version != 1 || actor.trim().is_empty() || reason.trim().is_empty() {
        return Err(invalid("Version 1, review actor and reason are required"));
    }
    if !proposal.events.is_empty() {
        return Err(invalid(
            "Resolve a pending proposal once; prepare a new revision to change the decision",
        ));
    }
    if proposal.expected_project_revision != revision
        || current != &proposal.before
        || hash(&before(&proposal.request.operation)?)? != proposal.before_sha256
        || hash(current)? != proposal.before_sha256
    {
        return Err(EngineError::new(
            "stale_revision",
            "Proposal evidence or project revision changed; prepare a new suggestion",
        ));
    }
    let mut updated = proposal.clone();
    let response = if approve {
        let mut request = proposal.request.clone();
        request.actor = actor.into();
        let response = crate::engine::execute(path, request, control)?;
        if semantic_review(&reviewed_value(&response.output)?, &proposal.before)
            != semantic_review(&proposal.proposed, &proposal.before)
        {
            return Err(invalid(
                "Proposal preview differs from the operation; prepare it again",
            ));
        }
        updated.applied = Some(serde_json::to_value(&response).map_err(invalid)?);
        Some(response)
    } else {
        None
    };
    updated.events.push(Event {
        actor: actor.into(),
        reason: reason.into(),
        decision: if approve { "approved" } else { "rejected" }.into(),
        unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    });
    Ok((updated, response))
}
/// Reversal is another reasoned review revision; old evidence and decisions survive.
pub fn reversal(proposal: &Proposal, actor: &str, reason: &str) -> Result<Request> {
    let response: Response = serde_json::from_value(
        proposal
            .applied
            .clone()
            .ok_or_else(|| invalid("Only applied proposals can be reversed"))?,
    )
    .map_err(invalid)?;
    let operation = match (&proposal.request.operation, response.output) {
        (
            Operation::ReviewTargeted {
                batch,
                sample,
                target,
                ..
            },
            Output::TargetedQuantification { batch: current },
        ) => {
            let row = batch
                .results
                .iter()
                .find(|row| row.sample == *sample && row.target == *target)
                .ok_or_else(|| invalid("Original target row missing"))?;
            Operation::ReviewTargeted {
                expected_revision: current.reviews.len(),
                batch: current,
                sample: sample.clone(),
                target: target.clone(),
                accepted: row.state != crate::targeted::State::Rejected,
                reason: reason.into(),
            }
        }
        (
            Operation::ReviewQc {
                report, rule_id, ..
            },
            Output::QcReport { report: current },
        ) => {
            let acknowledged = report
                .reviews
                .iter()
                .rev()
                .find(|event| event.rule_id == *rule_id)
                .map(|event| event.acknowledged)
                .unwrap_or(false);
            Operation::ReviewQc {
                expected_revision: current.reviews.len(),
                report: current,
                rule_id: rule_id.clone(),
                reason: reason.into(),
                acknowledged,
            }
        }
        (
            Operation::ReviseChromatogram { analysis, .. },
            Output::ChromatographicProcessing { mut analyses, .. },
        ) => {
            let current = analyses
                .pop()
                .ok_or_else(|| invalid("Applied analysis missing"))?;
            Operation::ReviseChromatogram {
                expected_revision: current.revisions.len(),
                analysis: Box::new(current),
                correction: crate::chromatography::Correction::Restore {
                    revision: analysis.revisions.len(),
                    reason: reason.into(),
                },
                preview: false,
            }
        }
        (
            Operation::ReviewFeatureAnnotation {
                hypothesis_id,
                ledger: original,
                ..
            },
            Output::FeatureAnnotations { ledger },
        ) => Operation::ReviewFeatureAnnotation {
            expected_revision: ledger.hypotheses.len() + ledger.reviews.len(),
            ledger,
            hypothesis_id: *hypothesis_id,
            reason: reason.into(),
            decision: original
                .reviews
                .iter()
                .rev()
                .find(|r| r.hypothesis_id == *hypothesis_id)
                .map_or(crate::annotation::Decision::Reopen, |r| r.decision),
        },
        (
            Operation::AnnotateSpectrum {
                report: original, ..
            },
            Output::SpectralSearch { report },
        ) => Operation::AnnotateSpectrum {
            expected_revision: report.annotations.len(),
            report,
            annotation: original
                .annotations
                .last()
                .cloned()
                .map(|mut annotation| {
                    annotation.reason = reason.into();
                    annotation.actor = actor.into();
                    annotation
                })
                .unwrap_or(crate::spectral::Annotation {
                    id: uuid::Uuid::nil(),
                    actor: actor.into(),
                    reason: reason.into(),
                    unix_ms: 0,
                    candidate_accession: None,
                    label: "Reversed annotation".into(),
                    confidence: crate::spectral::Confidence::Unknown,
                    evidence: vec![],
                }),
        },
        _ => {
            return Err(invalid(
                "Applied proposal output does not match its review operation",
            ))
        }
    };
    Ok(Request {
        version: 1,
        operation_id: Default::default(),
        actor: actor.into(),
        operation,
    })
}
