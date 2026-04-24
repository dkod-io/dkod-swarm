use dkod_mcp::error::Error as McpError;

#[test]
fn worktree_error_wraps() {
    let wt = dkod_worktree::Error::Invalid("boom".into());
    let err: McpError = wt.into();
    assert!(matches!(err, McpError::Worktree(_)));
    assert!(err.to_string().contains("boom"));
}
