//! Open Telmetery metrix sink parsing incoming oltp/http json & stores metrics in memory
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, StatusCode},
    response::{Html, IntoResponse},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::dashboard::AppState;

// shared state
pub type OtelStoreHandle = Arc<RwLock<OtelStore>>;

#[derive(Debug, Clone, Default)]
pub struct OtelStore {
    pub metrics: HashMap<String, OtelMetric>,
    pub batches_received: u64,
    pub last_received_at: Option<std::time::Instant>,
}

impl OtelStore {
    pub fn new_handle() -> OtelStoreHandle {
        Arc::new(RwLock::new(OtelStore::default()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelMetric {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub description: String,
    pub metric_type: String, // "gauge", "sum", "histogram"
    pub timestamp_ms: u64,
}

// JSON serialization for the API endpoint

#[derive(Serialize)]
struct ApiResponse {
    batches_received: u64,
    last_updated_secs_ago: Option<f64>,
    metrics: Vec<OtelMetric>,
}

// Handlers
/// POST /v1/metrics: Rx oltp json from the Open Telemetry Collector
pub async fn metrics_post_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> impl IntoResponse {
    let store = &state.otel_store;
    let content_type = req
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let body_bytes = match to_bytes(req.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    if let Ok(json) = serde_json::from_slice::<Value>(&body_bytes) {
        let parsed = parse_otlp_json(&json);
        let count = parsed.len();

        let mut s = store.write().await;
        s.batches_received += 1;
        s.last_received_at = Some(std::time::Instant::now());
        for m in parsed {
            s.metrics.insert(m.name.clone(), m);
        }

        println!(
            " [OTEL-SINK] Received OTLP metrics batch ({} bytes, type: {}, {} metrics parsed)",
            body_bytes.len(),
            content_type,
            count
        );
    } else {
        println!(" [OTEL-SINK] Warning: Received non-JSON payload on metrics endpoint");
    }

    StatusCode::OK
}

/// GET /otel:  serves Open Telemetery dashboard page
pub async fn otel_dashboard_handler() -> Html<&'static str> {
    Html(include_str!("opentele_dashboard.html"))
}

/// GET /otel/api/metrics: returns a json snapshot of all stored metrics
pub async fn otel_api_handler(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.otel_store.read().await;

    let last_updated_secs_ago = s.last_received_at.map(|t| t.elapsed().as_secs_f64());

    let mut metrics: Vec<OtelMetric> = s.metrics.values().cloned().collect();
    metrics.sort_by(|a, b| a.name.cmp(&b.name));

    let resp = ApiResponse {
        batches_received: s.batches_received,
        last_updated_secs_ago,
        metrics,
    };

    axum::Json(resp)
}

// oltp json parser
fn parse_otlp_json(root: &Value) -> Vec<OtelMetric> {
    let mut result = Vec::new();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let resource_metrics = match root.get("resourceMetrics").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return result,
    };

    for rm in resource_metrics {
        let scope_metrics = match rm.get("scopeMetrics").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => continue,
        };

        for sm in scope_metrics {
            let metrics_arr = match sm.get("metrics").and_then(|v| v.as_array()) {
                Some(arr) => arr,
                None => continue,
            };

            for metric in metrics_arr {
                let name = metric
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let unit = metric
                    .get("unit")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let description = metric
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // gauge -> sum -> histogram
                if let Some(gauge) = metric.get("gauge") {
                    if let Some(val) = extract_latest_data_point(gauge) {
                        result.push(OtelMetric {
                            name,
                            value: val,
                            unit,
                            description,
                            metric_type: "gauge".to_string(),
                            timestamp_ms: now_ms,
                        });
                    }
                } else if let Some(sum) = metric.get("sum") {
                    if let Some(val) = extract_latest_data_point(sum) {
                        result.push(OtelMetric {
                            name,
                            value: val,
                            unit,
                            description,
                            metric_type: "counter".to_string(),
                            timestamp_ms: now_ms,
                        });
                    }
                } else if let Some(histogram) = metric.get("histogram") {
                    // Hardcoded: for histograms taking the sum or count as representative value
                    if let Some(dp) = histogram
                        .get("dataPoints")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.last())
                    {
                        let val = dp
                            .get("sum")
                            .and_then(|v| v.as_f64())
                            .or_else(|| dp.get("count").and_then(|v| v.as_f64()))
                            .unwrap_or(0.0);
                        let count = dp.get("count").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let display_val = if count > 0.0 { val / count } else { val };

                        result.push(OtelMetric {
                            name,
                            value: display_val,
                            unit,
                            description,
                            metric_type: "histogram".to_string(),
                            timestamp_ms: now_ms,
                        });
                    }
                }
            }
        }
    }

    result
}

fn extract_latest_data_point(container: &Value) -> Option<f64> {
    let points = container.get("dataPoints")?.as_array()?;
    let dp = points.last()?;

    dp.get("asDouble")
        .and_then(|v| v.as_f64())
        .or_else(|| {
            dp.get("asInt")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
        })
        .or_else(|| dp.get("asInt").and_then(|v| v.as_f64()))
}
