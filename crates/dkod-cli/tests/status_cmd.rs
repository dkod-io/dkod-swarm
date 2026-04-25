#[tokio::test]
async fn status_prints_empty_response_when_no_session() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let status = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());
    dkod_worktree::init_repo(&root, None).unwrap();

    let out = dkod_cli::cmd::status::render(&root)
        .await
        .expect("status::render");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert!(parsed["active_session_id"].is_null());
    assert!(parsed["dk_branch"].is_null());
    assert_eq!(parsed["groups"].as_array().unwrap().len(), 0);
}
