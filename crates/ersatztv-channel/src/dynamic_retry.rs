use std::future::Future;
use std::time::Duration;

use rand::RngExt;
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use tokio::time::Instant;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_BACKOFF: Duration = Duration::from_millis(250);

// a shorter attempt can't finish a request
const MIN_ATTEMPT: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    pub timeout: Duration,
    pub budget: Duration,
    pub backoff: Duration,
}

impl RetryPolicy {
    pub fn new(
        timeout_us: Option<u64>,
        retry_budget_ms: Option<u64>,
        retry_backoff_ms: Option<u64>,
    ) -> RetryPolicy {
        RetryPolicy {
            timeout: timeout_us.map_or(DEFAULT_TIMEOUT, Duration::from_micros),
            budget: Duration::from_millis(retry_budget_ms.unwrap_or_default()),
            backoff: retry_backoff_ms
                .filter(|ms| *ms > 0)
                .map_or(DEFAULT_BACKOFF, Duration::from_millis),
        }
    }

    fn base_delay(&self, retry: u32) -> Duration {
        self.backoff
            .saturating_mul(2u32.saturating_pow(retry.saturating_sub(1)))
    }
}

#[derive(Debug)]
pub struct AttemptError {
    retryable: bool,
    message: String,
}

impl From<reqwest::Error> for AttemptError {
    fn from(e: reqwest::Error) -> Self {
        // a 4xx or a bad body is an answer, not an outage
        let retryable = match e.status() {
            Some(status) => status.is_server_error(),
            None => !(e.is_builder() || e.is_redirect() || e.is_decode()),
        };
        AttemptError {
            retryable,
            message: e.to_string(),
        }
    }
}

pub async fn get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    uri: &str,
    headers: &HeaderMap,
    timeout: Duration,
) -> Result<T, AttemptError> {
    Ok(client
        .get(uri)
        .headers(headers.clone())
        .timeout(timeout)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// The budget includes time spent in attempts.
pub async fn with_retry<T, F, Fut>(policy: &RetryPolicy, mut attempt: F) -> Result<T, String>
where
    F: FnMut(Duration) -> Fut,
    Fut: Future<Output = Result<T, AttemptError>>,
{
    if policy.budget.is_zero() {
        return attempt(policy.timeout).await.map_err(|e| e.message);
    }

    let started = Instant::now();
    let mut attempts = 0;
    loop {
        let left = policy.budget.saturating_sub(started.elapsed());
        attempts += 1;
        let err = match attempt(policy.timeout.min(left)).await {
            Ok(value) => return Ok(value),
            Err(err) => err,
        };

        let left = policy.budget.saturating_sub(started.elapsed());
        let delay = jitter(policy.base_delay(attempts));
        if !err.retryable || delay + MIN_ATTEMPT > left {
            let plural = if attempts == 1 { "" } else { "s" };
            return Err(format!(
                "{} (after {attempts} attempt{plural})",
                err.message
            ));
        }

        log::warn!(
            "dynamic source attempt {attempts} failed, retrying in {}ms: {}",
            delay.as_millis(),
            err.message
        );
        tokio::time::sleep(delay).await;
    }
}

fn jitter(delay: Duration) -> Duration {
    delay.mul_f64(rand::rng().random_range(0.5..=1.0))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use axum::Router;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::get;
    use serde_json::{Value, json};

    use super::*;

    async fn serve(responses: Vec<(StatusCode, &'static str)>) -> (String, Arc<AtomicU32>) {
        let hits = Arc::new(AtomicU32::new(0));
        let counter = hits.clone();
        let app = Router::new().route(
            "/",
            get(move || {
                let n = counter.fetch_add(1, Ordering::SeqCst) as usize;
                let (status, body) = responses[n.min(responses.len() - 1)];
                async move {
                    if body == "hang" {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                    Response::from((status, body).into_response())
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        (format!("http://{addr}/"), hits)
    }

    async fn fetch(uri: &str, policy: RetryPolicy) -> Result<Value, String> {
        let client = reqwest::Client::new();
        let headers = HeaderMap::new();
        with_retry(&policy, |timeout| {
            get_json::<Value>(&client, uri, &headers, timeout)
        })
        .await
    }

    fn policy(timeout_ms: u64, budget_ms: u64, backoff_ms: u64) -> RetryPolicy {
        RetryPolicy::new(Some(timeout_ms * 1000), Some(budget_ms), Some(backoff_ms))
    }

    const OK: (StatusCode, &str) = (StatusCode::OK, r#"{"ok":true}"#);

    #[test]
    fn policy_defaults() {
        assert_eq!(
            RetryPolicy::new(None, None, None),
            RetryPolicy {
                timeout: DEFAULT_TIMEOUT,
                budget: Duration::ZERO,
                backoff: DEFAULT_BACKOFF,
            }
        );
        assert_eq!(
            RetryPolicy::new(None, Some(1000), Some(0)).backoff,
            DEFAULT_BACKOFF
        );
    }

    #[test]
    fn backoff_doubles() {
        let policy = policy(1000, 10_000, 250);
        let delays: Vec<u128> = (1..=4).map(|n| policy.base_delay(n).as_millis()).collect();
        assert_eq!(delays, [250, 500, 1000, 2000]);
        assert!(policy.base_delay(100) > Duration::from_hours(24));
    }

    #[test]
    fn jitter_stays_within_half_to_full() {
        for _ in 0..100 {
            let d = jitter(Duration::from_millis(1000));
            assert!(d >= Duration::from_millis(500) && d <= Duration::from_millis(1000));
        }
    }

    #[tokio::test]
    async fn retries_5xx_until_success() {
        let (uri, hits) = serve(vec![
            (StatusCode::INTERNAL_SERVER_ERROR, ""),
            (StatusCode::SERVICE_UNAVAILABLE, ""),
            OK,
        ])
        .await;
        let value = fetch(&uri, policy(1000, 5000, 10)).await.unwrap();
        assert_eq!(value, json!({"ok": true}));
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn does_not_retry_4xx() {
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::NOT_FOUND,
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            let (uri, hits) = serve(vec![(status, ""), OK]).await;
            let err = fetch(&uri, policy(1000, 5000, 10)).await.unwrap_err();
            assert!(err.contains("after 1 attempt)"), "{status}: {err}");
            assert_eq!(hits.load(Ordering::SeqCst), 1, "{status}");
        }
    }

    #[tokio::test]
    async fn does_not_retry_invalid_body() {
        let (uri, hits) = serve(vec![(StatusCode::OK, "not json"), OK]).await;
        fetch(&uri, policy(1000, 5000, 10)).await.unwrap_err();
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn no_budget_makes_one_attempt() {
        let (uri, hits) = serve(vec![(StatusCode::INTERNAL_SERVER_ERROR, ""), OK]).await;
        let err = fetch(&uri, policy(1000, 0, 10)).await.unwrap_err();
        assert!(!err.contains("after"), "{err}");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn hanging_resolver_stays_inside_budget() {
        let (uri, hits) = serve(vec![(StatusCode::OK, "hang")]).await;
        let started = std::time::Instant::now();
        let err = fetch(&uri, policy(300, 1000, 10)).await.unwrap_err();
        let elapsed = started.elapsed();
        assert!(elapsed <= Duration::from_millis(1500), "{elapsed:?}");
        assert!(hits.load(Ordering::SeqCst) >= 3, "{err}");
    }

    #[tokio::test]
    async fn attempt_timeout_shrinks_to_budget_left() {
        let (uri, hits) = serve(vec![(StatusCode::OK, "hang")]).await;
        let started = std::time::Instant::now();
        fetch(&uri, policy(10_000, 500, 10)).await.unwrap_err();
        assert!(started.elapsed() <= Duration::from_millis(1000));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_connection_errors() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let uri = format!("http://{}/", listener.local_addr().unwrap());
        drop(listener);
        let err = fetch(&uri, policy(1000, 500, 10)).await.unwrap_err();
        assert!(!err.contains("after 1 attempt)"), "{err}");
    }
}
