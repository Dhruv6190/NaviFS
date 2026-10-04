//! Automated integration test executing full MCP tool trace against seeded sample directory.

#[tokio::test]
async fn test_automated_mcp_tool_trace() {
    let report = navifs_eval::run_evaluation()
        .await
        .expect("Full evaluation and tool trace must succeed");

    assert!(report.all_specs_passed);
    assert!(report.server_instructions_verified);
    assert_eq!(report.protocol_version, "2024-11-05");
    assert!(report.tools_registered.contains(&"search".to_string()));
    assert!(report.tools_registered.contains(&"inspect".to_string()));
    assert!(report.tools_registered.contains(&"open".to_string()));
    assert!(report.tools_registered.contains(&"related".to_string()));
    assert!(report.search_verified);
    assert!(report.inspect_verified);
    assert!(report.related_verified);
    assert!(report.open_verified);
}
