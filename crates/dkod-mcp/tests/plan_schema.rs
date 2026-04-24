use dkod_mcp::schema::{PlanRequest, PlanResponse};

#[test]
fn plan_request_round_trips() {
    let req = PlanRequest {
        task_prompt: "refactor auth".into(),
        in_scope: vec!["crate::auth::login".into(), "crate::auth::logout".into()],
        files: vec!["src/auth.rs".into()],
        target_groups: 2,
    };
    let j = serde_json::to_string(&req).unwrap();
    let back: PlanRequest = serde_json::from_str(&j).unwrap();
    assert_eq!(back.target_groups, 2);
    assert_eq!(back.in_scope.len(), 2);
}

#[test]
fn plan_response_is_serializable() {
    let resp = PlanResponse {
        session_preview_id: None,
        groups: vec![],
        warnings: vec![],
        unresolved_edges: 0,
    };
    let _ = serde_json::to_string(&resp).unwrap();
}
