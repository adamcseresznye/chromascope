#![cfg(feature = "mcp-headless")]
use chromascope::{domain::*, engine_mcp::EngineMcp, project::Project};
use rmcp::ServiceExt;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};
include!("fixtures/targeted_support.rs");

struct Client {
    read: BufReader<ReadHalf<DuplexStream>>,
    write: WriteHalf<DuplexStream>,
    id: u64,
}
impl Client {
    async fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        self.write
            .write_all(
                format!(
                    "{}\n",
                    json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.read.read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        serde_json::from_str(&line).unwrap()
    }
    async fn call(&mut self, name: &str, args: Value) -> Value {
        self.rpc("tools/call", json!({"name":name,"arguments":args}))
            .await
    }
    async fn start(&mut self, path: &str, operation: Operation) -> String {
        let request = Request {
            version: 1,
            operation_id: Default::default(),
            actor: "integration-test AI client".into(),
            operation,
        };
        let value = self
            .call("start_analysis", json!({"path":path,"request":request}))
            .await;
        assert_ne!(value["result"]["isError"], true, "{value}");
        value["result"]["structuredContent"]["job_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    async fn wait(&mut self, id: &str) {
        for _ in 0..500 {
            let value = self.call("analysis_status", json!({"job_id":id})).await;
            match value["result"]["structuredContent"]["state"]
                .as_str()
                .unwrap()
            {
                "succeeded" => return,
                "failed" | "cancelled" => panic!("{value}"),
                _ => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }
        panic!("job timeout")
    }
    async fn response(&mut self, id: &str) -> Value {
        self.wait(id).await;
        let value = self
            .call(
                "prepare_analysis_report",
                json!({"job_ids":[id],"explanation":"Client interpretation, not evidence"}),
            )
            .await;
        assert_ne!(value["result"]["isError"], true, "{value}");
        value["result"]["structuredContent"]["bundle"]["artifacts"][0]["response"].clone()
    }
}
async fn connect(server: EngineMcp) -> (Client, tokio::task::JoinHandle<()>) {
    let (client, transport) = tokio::io::duplex(1024 * 1024);
    let task = tokio::spawn(async move {
        server
            .serve(transport)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let (read, write) = tokio::io::split(client);
    let mut client = Client {
        read: BufReader::new(read),
        write,
        id: 0,
    };
    let initialized=client.rpc("initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"targeted workflow test","version":"1"}})).await;
    assert!(initialized["result"]["capabilities"]["tools"].is_object());
    client
        .write
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    (client, task)
}
#[test]
fn complete_targeted_workflow_review_qc_report_and_project_history() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let temp=tempfile::tempdir().unwrap();
        let batch=raw_request(temp.path());
        let source=batch.samples[0].source.clone();
        let root=temp.path().join("project");
        let mut project=Project::create(&root).unwrap();
        for sample in &batch.samples {project.register(std::path::Path::new(&sample.source)).unwrap();}
        project.commit(&root,0).unwrap();
        let dataset_id=project.sources[0].id.0.to_string();
        let (mut client,task)=connect(EngineMcp::new(vec![temp.path().to_owned()]).unwrap().with_project_writes(true)).await;
        let listed=client.rpc("tools/list",json!({})).await;
        let tools=listed["result"]["tools"].as_array().unwrap();
        for name in ["analytical_operation","start_untargeted","untargeted_features","start_analysis","commit_analysis"] {assert!(tools.iter().any(|t|t["name"]==name));}
        let schema=tools.iter().find(|t|t["name"]=="start_analysis").unwrap()["inputSchema"].to_string();
        for operation in ["targeted_batch","analyze_statistics","search_spectral_library","revise_chromatogram","evaluate_targeted_qc"] {assert!(schema.contains(operation));}
        let capabilities = client.call("analysis_capabilities", json!({})).await;
        assert_eq!(capabilities["result"]["structuredContent"]["workflows"].as_array().unwrap().len(),6);
        assert!(capabilities["result"]["structuredContent"]["response_schema"].is_object());
        let discovered=client.call("discover_datasets",json!({"directory":temp.path(),"offset":0,"limit":3})).await;
        assert_eq!(discovered["result"]["structuredContent"]["total"],8);
        assert_eq!(discovered["result"]["structuredContent"]["next_offset"],3);
        let id=client.start(&source,Operation::TargetedBatch{batch}).await;
        let response=client.response(&id).await;
        let batch:chromascope::targeted::BatchResult=serde_json::from_value(response["output"]["batch"].clone()).unwrap();
        let unknown=batch.results.iter().find(|r|r.sample=="raw6" && r.target=="a").unwrap();
        assert!((unknown.concentration.unwrap()-10.).abs()<1e-7);
        assert!(!unknown.reviewed);
        assert!(batch.results.iter().find(|r|r.sample=="raw7" && r.target=="a").unwrap().concentration.is_none());
        // A deliberately strict QC threshold raises a real calculated failure.
        let rule:chromascope::qc::Rule=serde_json::from_value(json!({"id":"strict_accuracy","target":"a","group":null,"role":"qc","batch":null,"metric":"accuracy","lower":99.9,"upper":100.1,"minimum_n":2,"required":true,"reference_group":null,"calibration":null})).unwrap();
        let qc_id=client.start(&source,Operation::EvaluateTargetedQc{batch:Box::new(batch.clone()),rules:vec![rule]}).await;
        let qc=client.response(&qc_id).await;
        assert!(qc["output"]["report"].to_string().contains("fail"));
        let review_id=client.start(&source,Operation::ReviewTargeted{batch:Box::new(batch),expected_revision:0,sample:"raw6".into(),target:"a".into(),accepted:true,reason:"Reviewer checked raw quantifier and qualifier against standards".into()}).await;
        let reviewed=client.response(&review_id).await;
        let reviewed_batch:chromascope::targeted::BatchResult=serde_json::from_value(reviewed["output"]["batch"].clone()).unwrap();
        assert!(reviewed_batch.results.iter().find(|r|r.sample=="raw6"&&r.target=="a").unwrap().reviewed);
        let csv_id=client.start(&source,Operation::ExportTargeted{batch:Box::new(reviewed_batch)}).await;
        let csv=client.response(&csv_id).await;
        assert!(csv["output"]["csv"].as_str().unwrap().contains("ng/mL"));
        let page=client.call("analysis_result",json!({"job_id":id,"pointer":"/output/batch/results","offset":0,"limit":1})).await;
        assert_eq!(page["result"]["structuredContent"]["items"].as_array().unwrap().len(),1);
        let bad=client.call("analysis_result",json!({"job_id":id,"pointer":"/missing","offset":0,"limit":1})).await;
        assert_eq!(bad["result"]["structuredContent"]["code"],"invalid_parameters");
        let commit_args=json!({"directory":root,"dataset_id":dataset_id,"job_id":review_id,"expected_revision":1,"approval_actor":"test human (client assertion)","reason":"Preserve reviewed result as draft evidence"});
        let committed=client.call("commit_analysis",commit_args.clone()).await;
        assert_eq!(committed["result"]["structuredContent"]["revision"],2);
        let stale=client.call("commit_analysis",commit_args).await;
        assert_eq!(stale["result"]["structuredContent"]["code"],"stale_revision");
        let saved=Project::open(&root).unwrap();
        assert_eq!(saved.agent_commits.len(),1);
        assert!(!saved.agent_commits[0].identity_verified);
        let report=client.call("prepare_analysis_report",json!({"job_ids":[id,qc_id,review_id,csv_id],"explanation":"Hypothesis about QC; not verified"})).await;
        let bundle=&report["result"]["structuredContent"]["bundle"];
        assert_eq!(bundle["status"],"review_draft");
        assert_eq!(bundle["ai_explanation"]["independently_verified"],false);
        assert_eq!(bundle["artifacts"].as_array().unwrap().len(),4);
        assert!(bundle["audit"].as_array().unwrap().iter().any(|v|v["tool"]=="commit_analysis"));
        use sha2::{Digest,Sha256};
        assert_eq!(report["result"]["structuredContent"]["bundle_sha256"],format!("{:x}",Sha256::digest(serde_json::to_vec(bundle).unwrap())));
        let restored=Project::restore_revision(&root,1,2).unwrap();
        assert_eq!(restored.agent_commits.len(),1);
        assert!(restored.results.is_empty());
        assert!(root.join("revision-00000000000000000002.json").exists());
        drop(client);task.await.unwrap();
    });
}
#[test]
fn transport_validation_authorization_cancellation_and_read_only_policy() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let temp=tempfile::tempdir().unwrap();
        let other=tempfile::tempdir().unwrap();
        let outside=other.path().join("source.mzML");std::fs::write(&outside,b"unchanged").unwrap();
        let path=temp.path().join("source");std::fs::write(&path,b"source identity anchor").unwrap();
        let (mut client,task)=connect(EngineMcp::new(vec![temp.path().to_owned()]).unwrap()).await;
        let denied=client.call("discover_datasets",json!({"directory":other.path(),"offset":0,"limit":1})).await;
        assert_eq!(denied["result"]["structuredContent"]["code"],"unauthorized");
        let invalid=client.call("start_analysis",json!({"path":path,"request":{"version":1,"operation_id":"invalid","actor":"AI","operation":{"operation":"metadata"}}})).await;
        assert!(invalid["error"].is_object()||invalid["result"]["isError"]==true);
        let id=client.start(path.to_str().unwrap(),Operation::Integrate{points:vec![[0.,0.],[1.,4.],[2.,0.]],start:Minutes(0.),end:Minutes(2.)}).await;
        let response=client.response(&id).await;
        assert_eq!(response["output"]["area"],4.0);
        let cancel=client.call("cancel_analysis",json!({"job_id":id})).await;
        assert_eq!(cancel["result"]["structuredContent"]["state"],"succeeded");
        let commit=client.call("commit_analysis",json!({"directory":temp.path(),"dataset_id":uuid::Uuid::new_v4().to_string(),"job_id":id,"expected_revision":0,"approval_actor":"AI says human","reason":"try bypass"})).await;
        assert_eq!(commit["result"]["structuredContent"]["code"],"unauthorized");
        let invalid_page=client.call("session_audit",json!({"offset":0,"limit":0})).await;
        assert_eq!(invalid_page["result"]["structuredContent"]["code"],"invalid_parameters");
        assert_eq!(std::fs::read(outside).unwrap(),b"unchanged");
        drop(client);task.await.unwrap();
    });
}

#[test]
fn preview_cannot_commit_and_stale_review_remains_failed_evidence() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        use chromascope::chromatography::{Analysis,Trace,Config,Baseline,Correction};
        let temp=tempfile::tempdir().unwrap();
        let raw=raw_request(temp.path());
        let source=raw.samples[0].source.clone();
        let root=temp.path().join("project");
        let mut project=Project::create(&root).unwrap();
        let dataset_id=project.register(std::path::Path::new(&source)).unwrap();
        project.commit(&root,0).unwrap();
        let (mut client,task)=connect(EngineMcp::new(vec![temp.path().to_owned()]).unwrap().with_project_writes(true)).await;
        let processed=client.start(&source,Operation::ProcessChromatograms{traces:vec![Trace::from_points("known triangle".into(),&[[0.,0.],[0.5,0.],[1.,4.],[1.5,0.],[2.,0.]])],config:Config{smoothing:None,baseline:Baseline::EndpointChord,minimum_snr:0.,..Default::default()}}).await;
        let response=client.response(&processed).await;
        let analysis:Analysis=serde_json::from_value(response["output"]["analyses"][0].clone()).unwrap();
        let original=serde_json::to_value(&analysis).unwrap();
        let correction=Correction::Manual{intervals:vec![[0.5,1.5]],reason:"Preview for reviewer".into()};
        let preview=client.start(&source,Operation::ReviseChromatogram{analysis:Box::new(analysis.clone()),expected_revision:0,correction:correction.clone(),preview:true}).await;
        let result=client.response(&preview).await;
        assert_eq!(result["output"]["preview"],true);
        let commit=client.call("commit_analysis",json!({"directory":root,"dataset_id":dataset_id.0.to_string(),"job_id":preview,"expected_revision":1,"approval_actor":"reviewer","reason":"Must not permit preview"})).await;
        assert_eq!(commit["result"]["structuredContent"]["code"],"invalid_parameters");
        assert_eq!(Project::open(&root).unwrap().revision,1);
        assert_eq!(serde_json::to_value(analysis.clone()).unwrap(),original);
        let failed=client.start(&source,Operation::ReviseChromatogram{analysis:Box::new(analysis),expected_revision:99,correction,preview:false}).await;
        let mut terminal=false;
        for _ in 0..100 {
            let status=client.call("analysis_status",json!({"job_id":failed})).await;
            if status["result"]["structuredContent"]["state"]=="failed" {terminal=true;break;}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(terminal);
        let invalid_job=client.start(&source,Operation::Integrate{points:vec![[1.,1.],[0.,0.]],start:Minutes(0.),end:Minutes(1.)}).await;
        let mut invalid_terminal=false;
        for _ in 0..100 {
            let status=client.call("analysis_status",json!({"job_id":invalid_job})).await;
            if status["result"]["structuredContent"]["state"]=="failed" {invalid_terminal=true;break;}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(invalid_terminal);
        let report=client.call("prepare_analysis_report",json!({"job_ids":[processed,preview,failed,invalid_job],"explanation":"No independent scientific assertion"})).await;
        assert_eq!(report["result"]["structuredContent"]["bundle"]["artifacts"][2]["error"]["code"],"stale_revision");
        assert_eq!(report["result"]["structuredContent"]["bundle"]["artifacts"][3]["error"]["code"],"invalid_parameters");
        assert_eq!(report["result"]["structuredContent"]["bundle"]["artifacts"][3]["state"],"failed");
        drop(client);task.await.unwrap();
    });
}

#[test]
fn actual_stdio_example_retains_replay_bundle_and_refuses_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "example smoke reference".into(),
        operation: Operation::TargetedBatch { batch },
    };
    let input = temp.path().join("request.json");
    std::fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
    let output = temp.path().join("new-bundle");
    let run = || {
        std::process::Command::new("python")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("examples/mcp_targeted_workflow.py"),
            )
            .arg(&input)
            .arg(&output)
            .arg("--server")
            .arg(env!("CARGO_BIN_EXE_chromascope-engine-mcp"))
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report = std::fs::read(output.join("report.json")).unwrap();
    let value: Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(value["bundle"]["status"], "review_draft");
    assert_eq!(value["bundle"]["artifacts"].as_array().unwrap().len(), 2);
    assert!(output.join("transcript.json").exists());
    assert!(std::fs::read_to_string(output.join("csv.csv"))
        .unwrap()
        .contains("ng/mL"));
    assert!(!run().status.success());
    assert_eq!(std::fs::read(output.join("report.json")).unwrap(), report);
}

#[tokio::test]
async fn configured_project_report_export_protocol() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    Project::create(&root).unwrap();
    let (mut client, task) = connect(EngineMcp::new(vec![temp.path().to_owned()]).unwrap()).await;
    let result = client
        .call(
            "export_project_report",
            json!({"directory":root,"config":chromascope::delivery::Config::default()}),
        )
        .await;
    assert_ne!(result["result"]["isError"], true, "{result}");
    assert_eq!(
        result["result"]["structuredContent"]["report"]["status"],
        "review_draft"
    );
    assert!(result["result"]["structuredContent"]["html"]
        .as_str()
        .unwrap()
        .contains("not_available"));
    drop(client);
    task.abort();
}
