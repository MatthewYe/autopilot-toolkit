use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct WorkflowDefinition {
    pub(crate) version: String,
    pub(crate) stages: Vec<WorkflowStage>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct WorkflowStage {
    pub(crate) id: String,
    pub(crate) executor: String,
    pub(crate) skill: Option<String>,
    pub(crate) checkpoint: String,
    pub(crate) next_action: String,
}

pub(crate) fn load_workflow() -> Result<WorkflowDefinition, String> {
    serde_json::from_str(crate::WORKFLOW_JSON).map_err(|err| format!("cannot parse workflow: {err}"))
}

pub(crate) fn initial_stage_states(workflow: &WorkflowDefinition) -> Result<Vec<Value>, String> {
    workflow
        .stages
        .iter()
        .map(|stage| {
            let state = if stage.id == "intake" {
                "completed"
            } else if stage.id == "clarification" {
                "active"
            } else {
                "pending"
            };
            Ok(json!({
                "id": stage.id,
                "state": state,
                "revision": if stage.id == "intake" { 1 } else { 0 },
                "authorized_action": authorized_action(stage),
            }))
        })
        .collect()
}

pub(crate) fn active_run_for_session(worktree: &Path, session_id: &str) -> Result<Option<String>, String> {
    let runs_dir = worktree.join(".distill/runs");
    if !runs_dir.exists() {
        return Ok(None);
    }
    let mut matches = Vec::new();
    for entry in fs::read_dir(&runs_dir)
        .map_err(|err| format!("cannot list runs in {}: {err}", runs_dir.display()))?
    {
        let entry = entry.map_err(|err| format!("cannot inspect run entry: {err}"))?;
        let state_path = entry.path().join("state.json");
        if !state_path.is_file() {
            continue;
        }
        let state: Value = serde_json::from_str(
            &fs::read_to_string(&state_path)
                .map_err(|err| format!("cannot read {}: {err}", state_path.display()))?,
        )
        .map_err(|err| format!("cannot parse {}: {err}", state_path.display()))?;
        if (state["state"] == "active" || state["state"] == "blocked")
            && state["session_binding"]["session_id"] == session_id
        {
            if let Some(run_id) = state["run_id"].as_str() {
                matches.push(run_id.to_string());
            }
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        _ => Err(format!(
            "multiple unfinished runs are bound to session {session_id}"
        )),
    }
}

pub(crate) fn workflow_stage(state: &Value, id: &str) -> Result<WorkflowStage, String> {
    let stages = state["workflow"]["stages"]
        .as_array()
        .ok_or("state workflow stages are invalid")?;
    for stage in stages {
        let parsed: WorkflowStage = serde_json::from_value(stage.clone())
            .map_err(|err| format!("invalid workflow stage snapshot: {err}"))?;
        if parsed.id == id {
            return Ok(parsed);
        }
    }
    Err(format!("workflow stage not found: {id}"))
}

pub(crate) fn next_stage_after(workflow: &WorkflowDefinition, id: &str) -> Result<WorkflowStage, String> {
    let index = workflow
        .stages
        .iter()
        .position(|stage| stage.id == id)
        .ok_or_else(|| format!("workflow stage not found: {id}"))?;
    workflow
        .stages
        .get(index + 1)
        .cloned()
        .ok_or_else(|| format!("workflow has no stage after {id}"))
}

pub(crate) fn next_stage_after_snapshot(state: &Value, id: &str) -> Result<Option<WorkflowStage>, String> {
    let stages = state["workflow"]["stages"]
        .as_array()
        .ok_or("state workflow stages are invalid")?;
    let index = stages
        .iter()
        .position(|stage| stage["id"] == id)
        .ok_or_else(|| format!("workflow stage not found: {id}"))?;
    stages
        .get(index + 1)
        .map(|stage| {
            serde_json::from_value(stage.clone())
                .map_err(|err| format!("invalid workflow stage snapshot: {err}"))
        })
        .transpose()
}

pub(crate) fn authorized_action(stage: &WorkflowStage) -> Value {
    json!({
        "type": stage.next_action,
        "executor": stage.executor,
        "skill": stage.skill,
        "stage": stage.id,
        "expected_checkpoint": stage.checkpoint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_workflow() -> WorkflowDefinition {
        WorkflowDefinition {
            version: "distill.v1".to_string(),
            stages: vec![
                WorkflowStage {
                    id: "intake".to_string(),
                    executor: "runner".to_string(),
                    skill: None,
                    checkpoint: "explicit-text-captured".to_string(),
                    next_action: "capture-text-requirement".to_string(),
                },
                WorkflowStage {
                    id: "clarification".to_string(),
                    executor: "skill".to_string(),
                    skill: Some("grill-with-docs".to_string()),
                    checkpoint: "clarification-complete".to_string(),
                    next_action: "invoke-skill".to_string(),
                },
                WorkflowStage {
                    id: "prd".to_string(),
                    executor: "skill".to_string(),
                    skill: Some("to-prd".to_string()),
                    checkpoint: "testing-seam-confirmed".to_string(),
                    next_action: "invoke-skill".to_string(),
                },
            ],
        }
    }

    fn test_state_value() -> Value {
        let wf = test_workflow();
        json!({
            "workflow": {
                "version": wf.version,
                "stages": wf.stages.iter().map(|s| json!({
                    "id": s.id,
                    "executor": s.executor,
                    "skill": s.skill,
                    "checkpoint": s.checkpoint,
                    "next_action": s.next_action,
                })).collect::<Vec<_>>(),
            }
        })
    }

    // --- workflow_stage ---

    #[test]
    fn test_workflow_stage_finds_intake() {
        let state = test_state_value();
        let stage = workflow_stage(&state, "intake").unwrap();
        assert_eq!(stage.id, "intake");
        assert_eq!(stage.executor, "runner");
        assert_eq!(stage.next_action, "capture-text-requirement");
    }

    #[test]
    fn test_workflow_stage_finds_clarification() {
        let state = test_state_value();
        let stage = workflow_stage(&state, "clarification").unwrap();
        assert_eq!(stage.id, "clarification");
        assert_eq!(stage.executor, "skill");
        assert_eq!(stage.skill.as_deref(), Some("grill-with-docs"));
    }

    #[test]
    fn test_workflow_stage_not_found() {
        let state = test_state_value();
        let result = workflow_stage(&state, "nonexistent");
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("not found"));
    }

    #[test]
    fn test_workflow_stage_invalid_state() {
        let state = json!({"workflow": {"stages": "not-an-array"}});
        let result = workflow_stage(&state, "intake");
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("invalid"));
    }

    #[test]
    fn test_workflow_stage_missing_workflow() {
        let state = json!({});
        let result = workflow_stage(&state, "intake");
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("invalid"));
    }

    // --- next_stage_after ---

    #[test]
    fn test_next_stage_after_intake() {
        let wf = test_workflow();
        let next = next_stage_after(&wf, "intake").unwrap();
        assert_eq!(next.id, "clarification");
    }

    #[test]
    fn test_next_stage_after_clarification() {
        let wf = test_workflow();
        let next = next_stage_after(&wf, "clarification").unwrap();
        assert_eq!(next.id, "prd");
    }

    #[test]
    fn test_next_stage_after_last_returns_err() {
        let wf = test_workflow();
        let result = next_stage_after(&wf, "prd");
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("no stage after"));
    }

    #[test]
    fn test_next_stage_after_not_found() {
        let wf = test_workflow();
        let result = next_stage_after(&wf, "nonexistent");
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("not found"));
    }

    // --- authorized_action ---

    #[test]
    fn test_authorized_action_structure() {
        let stage = WorkflowStage {
            id: "clarification".to_string(),
            executor: "skill".to_string(),
            skill: Some("grill-with-docs".to_string()),
            checkpoint: "clarification-complete".to_string(),
            next_action: "invoke-skill".to_string(),
        };
        let action = authorized_action(&stage);
        assert_eq!(action["type"], "invoke-skill");
        assert_eq!(action["executor"], "skill");
        assert_eq!(action["skill"], "grill-with-docs");
        assert_eq!(action["stage"], "clarification");
        assert_eq!(action["expected_checkpoint"], "clarification-complete");
    }

    #[test]
    fn test_authorized_action_null_skill() {
        let stage = WorkflowStage {
            id: "intake".to_string(),
            executor: "runner".to_string(),
            skill: None,
            checkpoint: "explicit-text-captured".to_string(),
            next_action: "capture-text-requirement".to_string(),
        };
        let action = authorized_action(&stage);
        assert_eq!(action["type"], "capture-text-requirement");
        assert_eq!(action["executor"], "runner");
        assert!(action["skill"].is_null());
    }

    // --- load_workflow ---

    #[test]
    fn test_load_workflow_succeeds() {
        let wf = load_workflow().unwrap();
        assert_eq!(wf.version, "distill.v1");
        assert!(!wf.stages.is_empty());
    }

    // --- initial_stage_states ---

    #[test]
    fn test_initial_stage_states() {
        let wf = test_workflow();
        let states = initial_stage_states(&wf).unwrap();
        assert_eq!(states.len(), 3);

        // intake is completed with revision 1
        let intake = &states[0];
        assert_eq!(intake["id"], "intake");
        assert_eq!(intake["state"], "completed");
        assert_eq!(intake["revision"], 1);

        // clarification is active with revision 0
        let clarification = &states[1];
        assert_eq!(clarification["id"], "clarification");
        assert_eq!(clarification["state"], "active");
        assert_eq!(clarification["revision"], 0);

        // prd is pending with revision 0
        let prd = &states[2];
        assert_eq!(prd["id"], "prd");
        assert_eq!(prd["state"], "pending");
        assert_eq!(prd["revision"], 0);

        // all have authorized_action
        for state in &states {
            assert!(state["authorized_action"].is_object());
        }
    }
}
