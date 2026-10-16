//! MCP stdio runner. Deliberately a console binary on Windows.
use chromascope::{
    gui::MzViewerApp,
    mcp::{AccessPolicy, McpServer},
};
use rmcp::ServiceExt;
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error"))
        .target(env_logger::Target::Stderr)
        .init();
    let mut roots = vec![];
    let mut changes = false;
    let mut exports = false;
    let mut project_writes = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                eprintln!("Usage: chromascope-mcp --allow-root DIRECTORY [--allow-root DIRECTORY ...] [--allow-changes] [--allow-exports] [--allow-project-writes]\nHosts a live Chromascope window and local MCP stdio server with every viewer, quantification, QC and project tool. Roots must exist. Changes, exports and project writes are disabled by default. Logs go to stderr.");
                return Ok(());
            }
            "--allow-root" => roots.push(PathBuf::from(
                args.next().ok_or("--allow-root needs a path")?,
            )),
            "--allow-changes" => changes = true,
            "--allow-exports" => exports = true,
            "--allow-project-writes" => project_writes = true,
            _ => return Err(format!("Unknown option: {arg}").into()),
        }
    }
    let policy = AccessPolicy::new(roots, changes, exports)?.with_project_writes(project_writes);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 820.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Chromascope — MCP",
        options,
        Box::new(move |cc| {
            let mut app = MzViewerApp::new(cc);
            let (server, bridge) =
                McpServer::new(cc.egui_ctx.clone(), policy).map_err(|e| e.to_string())?;
            app.attach_mcp(bridge);
            let context = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Runtime::new().expect("MCP runtime");
                runtime.block_on(async {
                    match server.serve(rmcp::transport::stdio()).await {
                        Ok(service) => {
                            if let Err(e) = service.waiting().await {
                                log::error!("MCP: {e}");
                            }
                        }
                        Err(e) => log::error!("MCP initialization: {e}"),
                    }
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                });
            });
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
