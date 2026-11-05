//! Resolvers depend on this behavior. On failure, update `DynamicSource` in `schema/playout.json`
//! and bump the playout schema (AGENTS.md "Dynamic source callback").

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::get;
use ersatztv_channel::dynamic::{CALLBACK_HEADERS, resolve};
use ersatztv_channel::error::ChannelError;
use ersatztv_playout::playout::{PlayoutItem, PlayoutItemSource, SCHEMA};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::datetime;

const BUMP: &str =
    "the dynamic callback contract changed; see AGENTS.md \"Dynamic source callback\"";

const PLACEHOLDER_START: &str = "2026-01-01T12:00:00-06:00";
const PLACEHOLDER_FINISH: &str = "2026-01-01T13:00:00-06:00";
const NOW: OffsetDateTime = datetime!(2026-01-01 12:10:00.5 -06:00);

type Requests = Arc<Mutex<Vec<(Method, HeaderMap)>>>;

struct Resolver {
    uri: String,
    requests: Requests,
}

impl Resolver {
    fn hits(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

async fn serve(responses: Vec<(StatusCode, String)>) -> Resolver {
    let requests: Requests = Arc::default();
    let recorded = requests.clone();
    let app = Router::new().route(
        "/",
        get(move |method: Method, headers: HeaderMap| {
            let n = {
                let mut recorded = recorded.lock().unwrap();
                recorded.push((method, headers));
                recorded.len() - 1
            };
            let response = responses[n.min(responses.len() - 1)].clone();
            async move { response }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Resolver {
        uri: format!("http://{addr}/"),
        requests,
    }
}

async fn serve_item(item: Value) -> Resolver {
    serve(vec![(StatusCode::OK, item.to_string())]).await
}

fn placeholder(uri: &str, source: Value) -> PlayoutItem {
    let mut source = source;
    source["source_type"] = json!("dynamic");
    source["uri"] = json!(uri);
    serde_json::from_value(json!({
        "id": "placeholder",
        "start": PLACEHOLDER_START,
        "finish": PLACEHOLDER_FINISH,
        "source": source,
    }))
    .unwrap()
}

async fn resolve_with(resolver: &Resolver, source: Value) -> Result<PlayoutItem, ChannelError> {
    resolve(
        &reqwest::Client::new(),
        "5",
        &NOW,
        &placeholder(&resolver.uri, source),
    )
    .await
}

async fn resolve_item(item: Value) -> Result<PlayoutItem, ChannelError> {
    resolve_with(&serve_item(item).await, json!({})).await
}

/// Ten minutes, starting away from `NOW` so the start rewrite is exercised.
fn item(extra: Value) -> Value {
    let mut item = json!({
        "id": "resolved",
        "start": "2026-01-01T18:00:00Z",
        "finish": "2026-01-01T18:10:00Z",
        "source": { "source_type": "lavfi", "params": "testsrc" },
    });
    for (key, value) in extra.as_object().unwrap() {
        item[key] = value.clone();
    }
    item
}

fn local() -> Value {
    json!({ "source_type": "local", "path": "/media/a.mkv" })
}

fn dynamic() -> Value {
    json!({ "source_type": "dynamic", "uri": "http://127.0.0.1:1/" })
}

#[tokio::test]
async fn request() {
    let resolver = serve_item(item(json!({}))).await;
    resolve_with(
        &resolver,
        json!({
            "headers": ["X-Custom: value", "x-etv-channel: spoofed", "not a header"],
            "user_agent": "resolver-test",
        }),
    )
    .await
    .unwrap();

    let requests = resolver.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let (method, headers) = &requests[0];
    assert_eq!(method, Method::GET, "{BUMP}");
    assert_eq!(headers["x-custom"], "value");
    assert_eq!(headers["user-agent"], "resolver-test");

    let mut etv: Vec<(String, String)> = headers
        .iter()
        .filter(|(name, _)| name.as_str().starts_with("x-etv-"))
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_owned()))
        .collect();
    etv.sort();
    let schema = SCHEMA.uri();
    let expected = [
        ("x-etv-channel", "5"),
        ("x-etv-dynamic-id", "placeholder"),
        ("x-etv-now", "2026-01-01T12:10:00.5-06:00"),
        ("x-etv-playout-schema", schema.as_str()),
        ("x-etv-until", PLACEHOLDER_FINISH),
    ]
    .map(|(name, value)| (name.to_owned(), value.to_owned()));
    assert_eq!(etv, expected, "{BUMP}");

    let mut listed = CALLBACK_HEADERS.map(String::from).to_vec();
    listed.sort();
    let sent: Vec<String> = etv.into_iter().map(|(name, _)| name).collect();
    assert_eq!(listed, sent, "CALLBACK_HEADERS must list every header sent");
}

#[tokio::test]
async fn start_moves_to_now_and_keeps_duration() {
    let resolved = resolve_item(item(json!({}))).await.unwrap();
    assert_eq!(resolved.id, "resolved");
    assert_eq!(resolved.start, NOW, "{BUMP}");
    assert_eq!(
        resolved.finish,
        datetime!(2026-01-01 12:20:00.5 -06:00),
        "{BUMP}"
    );
}

#[tokio::test]
async fn finish_clamps_to_until() {
    let resolved = resolve_item(item(json!({ "finish": "2026-01-02T18:00:00Z" })))
        .await
        .unwrap();
    assert_eq!(resolved.start, NOW, "{BUMP}");
    assert_eq!(
        resolved.finish,
        datetime!(2026-01-01 13:00 -06:00),
        "{BUMP}"
    );
}

#[tokio::test]
async fn every_source_kind_loads() {
    let sources = [
        local(),
        json!({ "source_type": "lavfi", "params": "testsrc" }),
        json!({ "source_type": "http", "uri": "http://example.com/a.ts", "is_live": true }),
        json!({ "source_type": "rtsp", "uri": "rtsp://example.com/live" }),
        json!({ "source_type": "script", "command": "yt-dlp", "args": ["-o", "-"] }),
    ];
    for source in sources {
        let kind = source["source_type"].clone();
        resolve_item(item(json!({ "source": source })))
            .await
            .unwrap_or_else(|e| panic!("{kind} refused: {e}; {BUMP}"));
    }
}

#[tokio::test]
async fn tracks_watermark_and_graphics_load() {
    let resolved = resolve_item(item(json!({
        "source": null,
        "tracks": {
            "video": { "source": local(), "stream_index": 0 },
            "audio": { "source": { "source_type": "lavfi", "params": "anullsrc" } },
        },
        "watermark": { "source": local(), "location": "bottom_right" },
        "graphics": [{ "source": local(), "location": "top_left" }],
    })))
    .await;
    resolved.unwrap_or_else(|e| panic!("{e}; {BUMP}"));
}

#[tokio::test]
async fn unknown_fields_are_ignored() {
    resolve_item(item(json!({
        "from_a_newer_schema": true,
        "source": { "source_type": "lavfi", "params": "testsrc", "also_new": 1 },
    })))
    .await
    .unwrap_or_else(|e| panic!("{e}; {BUMP}"));
}

#[tokio::test]
async fn bad_probe_hint_is_dropped() {
    let mut source = local();
    source["probe_hint"] = json!({ "video": [{ "codec": "h264" }] });
    let resolved = resolve_item(item(json!({ "source": source })))
        .await
        .unwrap_or_else(|e| panic!("{e}; {BUMP}"));
    let Some(PlayoutItemSource::Local { probe_hint, .. }) = resolved.source else {
        panic!("expected a local source");
    };
    assert_eq!(probe_hint, None);
}

#[tokio::test]
async fn dynamic_sources_are_refused() {
    let items = [
        item(json!({ "source": dynamic() })),
        item(json!({ "tracks": { "audio": { "source": dynamic() } } })),
        item(json!({ "tracks": { "video": { "source": dynamic() } } })),
        item(json!({ "tracks": { "subtitle": { "source": dynamic() } } })),
        item(json!({ "watermark": { "source": dynamic(), "location": "bottom_right" } })),
        item(json!({ "graphics": [{ "source": dynamic(), "location": "top_left" }] })),
    ];
    for item in items {
        let result = resolve_item(item.clone()).await;
        assert!(
            matches!(result, Err(ChannelError::DynamicSourceCannotRecurse)),
            "{item}: {result:?}; {BUMP}"
        );
    }
}

#[tokio::test]
async fn item_without_duration_is_refused() {
    let result = resolve_item(item(json!({ "finish": "2026-01-01T18:00:00Z" }))).await;
    assert!(
        matches!(result, Err(ChannelError::DynamicSourceNoRemainingTime)),
        "{result:?}; {BUMP}"
    );
}

#[tokio::test]
async fn invalid_items_are_refused() {
    let bodies = [
        String::from("not json"),
        json!({ "start": "2026-01-01T18:00:00Z", "finish": "2026-01-01T18:10:00Z" }).to_string(),
        item(json!({ "source": { "source_type": "local" } })).to_string(),
    ];
    for body in bodies {
        let resolver = serve(vec![(StatusCode::OK, body.clone())]).await;
        let result = resolve_with(&resolver, json!({ "retry_budget_ms": 2000 })).await;
        assert!(
            matches!(result, Err(ChannelError::DynamicSourceFailure(_))),
            "{body}: {result:?}; {BUMP}"
        );
        assert_eq!(
            resolver.hits(),
            1,
            "{body}: an invalid item is not retried; {BUMP}"
        );
    }
}

#[tokio::test]
async fn server_errors_retry_and_client_errors_do_not() {
    let ok = (StatusCode::OK, item(json!({})).to_string());
    let retry = json!({ "retry_budget_ms": 5000, "retry_backoff_ms": 10 });

    let resolver = serve(vec![
        (StatusCode::SERVICE_UNAVAILABLE, String::new()),
        ok.clone(),
    ])
    .await;
    resolve_with(&resolver, retry.clone())
        .await
        .unwrap_or_else(|e| panic!("{e}; {BUMP}"));
    assert_eq!(resolver.hits(), 2, "{BUMP}");

    let resolver = serve(vec![(StatusCode::NOT_FOUND, String::new()), ok]).await;
    let result = resolve_with(&resolver, retry).await;
    assert!(
        matches!(result, Err(ChannelError::DynamicSourceFailure(_))),
        "{result:?}; {BUMP}"
    );
    assert_eq!(resolver.hits(), 1, "{BUMP}");
}
