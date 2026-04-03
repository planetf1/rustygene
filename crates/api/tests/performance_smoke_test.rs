mod common;

use std::time::Instant;

use reqwest::StatusCode;
use serde_json::Value;

use common::spawn_test_server_with_kennedy_import;

#[tokio::test]
async fn import_and_search_performance_smoke() {
    let harness = spawn_test_server_with_kennedy_import().await;
    let base = harness.base_url.as_str();
    let client = &harness.client;

    // Import completion is guaranteed by harness; verify data is queryable.
    let probe = client
        .get(format!("{base}/api/v1/search?q=Kennedy&type=person"))
        .send()
        .await
        .expect("search probe");
    assert_eq!(probe.status(), StatusCode::OK);

    let mut latencies_ms = Vec::new();
    for _ in 0..40 {
        let started = Instant::now();
        let response = client
            .get(format!("{base}/api/v1/search?q=Kennedy&type=person"))
            .send()
            .await
            .expect("search request");
        assert_eq!(response.status(), StatusCode::OK);

        let body: Value = response.json().await.expect("search response body");
        let count = body
            .get("results")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        assert!(count > 5, "expected search to return many Kennedy matches");

        latencies_ms.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_index = ((latencies_ms.len() as f64) * 0.95).ceil() as usize - 1;
    let p95 = latencies_ms[p95_index.min(latencies_ms.len() - 1)];

    eprintln!("search latency p95_ms={p95:.2}");

    // Local-only smoke threshold; generous to avoid CI flakiness.
    assert!(
        p95 < 250.0,
        "expected local search p95 < 250ms, got {p95:.2}ms"
    );

    harness.shutdown().await;
}
