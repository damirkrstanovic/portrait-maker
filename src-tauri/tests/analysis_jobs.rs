use portrait_core::{
    Library,
    types::{JobState, Page, Query},
};
use portrait_manager_lib::state::{AnalysisRequest, DesktopState};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    time::{Duration, Instant},
};

fn library(root: &std::path::Path) -> Library {
    let lib = Library::create(root).unwrap();
    let source = uuid::Uuid::new_v4();
    let id = uuid::Uuid::new_v4();
    lib.connection()
        .execute(
            "INSERT INTO sources(id,name,kind) VALUES (?1,'Mock','folder')",
            [source.to_string()],
        )
        .unwrap();
    lib.connection()
        .execute(
            "INSERT INTO portraits(id,source_id,name,original_folder) VALUES (?1,?2,'Test','test')",
            [id.to_string(), source.to_string()],
        )
        .unwrap();
    let relative = format!("portraits/{id}/Fulllength.png");
    std::fs::create_dir_all(root.join(format!("portraits/{id}"))).unwrap();
    image::RgbaImage::from_pixel(2, 3, image::Rgba([10, 20, 30, 255]))
        .save(root.join(&relative))
        .unwrap();
    let size = std::fs::metadata(root.join(&relative)).unwrap().len();
    lib.connection().execute("INSERT INTO assets(portrait_id,role,relative_path,width,height,file_size) VALUES (?1,'large',?2,2,3,?3)",[id.to_string(),relative,size.to_string()]).unwrap();
    lib
}
fn finish(state: &DesktopState, id: uuid::Uuid) -> portrait_core::types::Job {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let job = state.job(id).unwrap().unwrap();
        if job.state != JobState::Running {
            return job;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn server(
    status: &str,
    body: String,
) -> (String, mpsc::Receiver<String>, std::thread::JoinHandle<()>) {
    server_with_gate(status, body, None)
}
fn server_with_gate(
    status: &str,
    body: String,
    gate: Option<mpsc::Receiver<()>>,
) -> (String, mpsc::Receiver<String>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let (tx, rx) = mpsc::channel();
    let status = status.to_owned();
    let join = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut data = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..n]);
            if let Some(end) = data.windows(4).position(|x| x == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..end]);
                let len: usize = headers
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|x| x.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + len {
                    break;
                }
            }
        }
        tx.send(String::from_utf8(data).unwrap()).unwrap();
        if let Some(gate) = gate {
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    (endpoint, rx, join)
}
#[test]
fn analysis_posts_only_large_image_saves_metadata_and_never_persists_token() {
    let temp = tempfile::tempdir().unwrap();
    let state = DesktopState::new(temp.path().join("settings"));
    state
        .activate(library(&temp.path().join("library")))
        .unwrap();
    let key = temp.path().join(".apikey");
    std::fs::write(&key, "mock-private-key\n").unwrap();
    let content =
        serde_json::json!({"description":"An armored fantasy warrior.","labels":[]}).to_string();
    let (endpoint, requests, server) = server(
        "200 OK",
        serde_json::json!({"choices":[{"message":{"content":content}}]}).to_string(),
    );
    let job = state
        .start_analysis(AnalysisRequest {
            endpoint: endpoint.clone(),
            model: "gemma-4-26b-a4b".into(),
            api_key_path: key.to_string_lossy().into(),
            selected_only: false,
            overwrite: false,
        })
        .unwrap();
    let request = requests.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(request.contains("Bearer mock-private-key"));
    assert!(request.contains("data:image/png;base64,"));
    let result = finish(&state, job.id);
    assert_eq!(result.state, JobState::Done, "{}", result.message);
    assert!(result.message.contains("1 analyzed"));
    let page = state
        .query_catalog(
            &Query::default(),
            Page {
                offset: 0,
                limit: 10,
            },
        )
        .unwrap();
    assert_eq!(
        page.items[0].model_description.as_deref(),
        Some("An armored fantasy warrior.")
    );
    let settings = std::fs::read_to_string(temp.path().join("settings/settings.json")).unwrap();
    assert!(!settings.contains("mock-private-key"));
    assert_eq!(state.analysis_settings().unwrap().endpoint, endpoint);
    server.join().unwrap();
}
#[test]
fn analysis_auth_failure_logs_response_and_redacts_token() {
    let temp = tempfile::tempdir().unwrap();
    let state = DesktopState::new(temp.path().join("settings"));
    state
        .activate(library(&temp.path().join("library")))
        .unwrap();
    let key = temp.path().join("key");
    std::fs::write(&key, "mock-private-key").unwrap();
    let (endpoint, requests, server) = server(
        "401 Unauthorized",
        r#"{"error":"\u006dock-private-key sensitive response","nested":"{\"token\":\"mock-private-key\"}"}"#.into(),
    );
    let job = state
        .start_analysis(AnalysisRequest {
            endpoint,
            model: "test".into(),
            api_key_path: key.to_string_lossy().into(),
            selected_only: false,
            overwrite: false,
        })
        .unwrap();
    requests.recv_timeout(Duration::from_secs(5)).unwrap();
    let result = finish(&state, job.id);
    assert_eq!(result.state, JobState::Failed);
    assert!(!result.message.contains("mock-private-key"));
    assert!(!result.message.contains("sensitive response"));
    assert!(result.message.contains("HTTP 401"));
    assert!(result.message.contains("analysis-errors.jsonl"));
    let log =
        std::fs::read_to_string(temp.path().join("settings/logs/analysis-errors.jsonl")).unwrap();
    let record: serde_json::Value = serde_json::from_str(log.trim()).unwrap();
    assert_eq!(record["httpStatus"], 401);
    assert_eq!(record["stage"], "http_status");
    let body: serde_json::Value =
        serde_json::from_str(record["response"].as_str().unwrap()).unwrap();
    assert_eq!(body["error"], "[REDACTED] sensitive response");
    assert!(!body.to_string().contains("mock-private-key"));
    assert!(!log.contains("Bearer"));
    server.join().unwrap();
}

#[test]
fn malformed_and_unsavable_responses_log_full_body_and_exact_error() {
    let malformed = format!("not JSON {}", "x".repeat(1024 * 1024 + 10));
    let invalid_metadata = serde_json::json!({"choices":[{"message":{"content":
        serde_json::json!({"description":"   ","labels":[]}).to_string()
    }}]})
    .to_string();
    for (body, stage, error_fragment, break_log) in [
        (malformed, "response_json", "expected ident", false),
        (invalid_metadata.clone(), "save", "description", false),
        (invalid_metadata, "save", "description", true),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let state = DesktopState::new(temp.path().join("settings"));
        state
            .activate(library(&temp.path().join("library")))
            .unwrap();
        let key = temp.path().join("key");
        std::fs::write(&key, "mock-private-key").unwrap();
        if break_log {
            std::fs::write(temp.path().join("settings/logs"), "not a directory").unwrap();
        }
        let (endpoint, requests, server) = server("200 OK", body.clone());
        let job = state
            .start_analysis(AnalysisRequest {
                endpoint,
                model: "test".into(),
                api_key_path: key.to_string_lossy().into(),
                selected_only: false,
                overwrite: false,
            })
            .unwrap();
        requests.recv_timeout(Duration::from_secs(5)).unwrap();
        let result = finish(&state, job.id);
        assert_eq!(result.state, JobState::Done);
        assert!(
            result.message.contains("0 analyzed, 1 failed"),
            "{}",
            result.message
        );
        assert!(
            result.message.contains(error_fragment),
            "{}",
            result.message
        );
        if break_log {
            assert!(
                result.message.contains("Could not write failure log"),
                "{}",
                result.message
            );
        } else {
            let log =
                std::fs::read_to_string(temp.path().join("settings/logs/analysis-errors.jsonl"))
                    .unwrap();
            let record: serde_json::Value = serde_json::from_str(log.trim()).unwrap();
            assert_eq!(record["stage"], stage);
            assert_eq!(record["response"], body);
            assert_eq!(record["jobId"], job.id.to_string());
            assert!(record["portraitId"].as_str().is_some());
            assert_eq!(record["httpStatus"], 200);
            assert!(record["error"].as_str().unwrap().contains(error_fragment));
        }
        server.join().unwrap();
    }
}

#[test]
fn cancellation_and_library_switch_during_http_never_commit_a_result() {
    for cancel in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let state = DesktopState::new(temp.path().join("settings"));
        state
            .activate(library(&temp.path().join("library")))
            .unwrap();
        let key = temp.path().join("key");
        std::fs::write(&key, "test-secret").unwrap();
        let content =
            serde_json::json!({"description":"Should not be saved.","labels":[]}).to_string();
        let (release, gate) = mpsc::channel();
        let (endpoint, requests, server) = server_with_gate(
            "200 OK",
            serde_json::json!({"choices":[{"message":{"content":content}}]}).to_string(),
            Some(gate),
        );
        let job = state
            .start_analysis(AnalysisRequest {
                endpoint,
                model: "test".into(),
                api_key_path: key.to_string_lossy().into(),
                selected_only: false,
                overwrite: false,
            })
            .unwrap();
        requests.recv_timeout(Duration::from_secs(5)).unwrap();
        // The request is still blocked: these library operations must not wait for HTTP.
        assert_eq!(
            state
                .query_catalog(
                    &Query::default(),
                    Page {
                        offset: 0,
                        limit: 10
                    }
                )
                .unwrap()
                .total,
            1
        );
        if cancel {
            state.cancel_job(job.id).unwrap();
        } else {
            state.close().unwrap();
            state
                .activate(Library::create(&temp.path().join("other")).unwrap())
                .unwrap();
        }
        release.send(()).unwrap();
        let result = finish(&state, job.id);
        assert_eq!(
            result.state,
            if cancel {
                JobState::Cancelled
            } else {
                JobState::Failed
            }
        );
        state.close().unwrap();
        let old = Library::open(&temp.path().join("library")).unwrap();
        assert_eq!(
            old.connection()
                .query_row("SELECT COUNT(*) FROM portrait_analysis", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        server.join().unwrap();
    }
}

// Explicit opt-in smoke test against a real server; always uses a disposable library.
#[test]
#[ignore = "requires PORTRAIT_VLM_IMAGE and PORTRAIT_VLM_KEY; sends one image to the configured server"]
fn live_model_describes_one_large_portrait_and_indexes_it() {
    let image = std::env::var("PORTRAIT_VLM_IMAGE").expect("set PORTRAIT_VLM_IMAGE");
    let key = std::env::var("PORTRAIT_VLM_KEY").expect("set PORTRAIT_VLM_KEY to a token file path");
    let temp = tempfile::tempdir().unwrap();
    let lib = library(&temp.path().join("library"));
    let relative: String = lib
        .connection()
        .query_row(
            "SELECT relative_path FROM assets WHERE role='large'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::copy(image, lib.root().join(relative)).unwrap();
    let state = DesktopState::new(temp.path().join("settings"));
    state.activate(lib).unwrap();
    let config = state.analysis_settings().unwrap();
    let job = state
        .start_analysis(AnalysisRequest {
            endpoint: std::env::var("PORTRAIT_VLM_ENDPOINT").unwrap_or(config.endpoint),
            model: config.model,
            api_key_path: key,
            selected_only: false,
            overwrite: false,
        })
        .unwrap();
    let started = Instant::now();
    loop {
        let result = state.job(job.id).unwrap().unwrap();
        if result.state != JobState::Running {
            assert_eq!(result.state, JobState::Done, "{}", result.message);
            assert!(
                result.message.contains("1 analyzed, 0 failed"),
                "{}",
                result.message
            );
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(150));
        std::thread::sleep(Duration::from_millis(100));
    }
    let page = state
        .query_catalog(
            &Query::default(),
            Page {
                offset: 0,
                limit: 1,
            },
        )
        .unwrap();
    let description = page.items[0].model_description.as_ref().unwrap();
    let word = description
        .split(|c: char| !c.is_alphanumeric())
        .find(|s| s.len() > 4)
        .unwrap();
    let found = state
        .query_catalog(
            &Query {
                text: word.into(),
                ..Query::default()
            },
            Page {
                offset: 0,
                limit: 1,
            },
        )
        .unwrap();
    assert_eq!(found.total, 1);
    println!(
        "Live analysis and FTS succeeded in {:.1}s; {} labels. Description: {}",
        started.elapsed().as_secs_f64(),
        page.items[0].labels.len(),
        description
    );
}
