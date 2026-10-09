//! Separate headless stdio MCP runner, preserving the existing desktop MCP executable.
use rmcp::ServiceExt;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut roots = Vec::new();
    let mut project_writes = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--allow-project-writes" => project_writes = true,
            "--allow-root" => roots.push(std::path::PathBuf::from(
                args.next().ok_or("--allow-root needs a path")?,
            )),
            "--help" | "-h" => {
                eprintln!(
                    "chromascope-engine-mcp --allow-root DIRECTORY [--allow-root DIRECTORY ...] [--allow-project-writes]"
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown option {arg}").into()),
        }
    }
    let server =
        chromascope::engine_mcp::EngineMcp::new(roots)?.with_project_writes(project_writes);
    tokio::runtime::Runtime::new()?.block_on(async {
        server
            .serve(rmcp::transport::stdio())
            .await?
            .waiting()
            .await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    })
}
