use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ffpipeline::input::{HttpInputSource, InputSource};
use ffpipeline::pipeline::{Pipeline, StillImageBake};
use ffpipeline::still_image_cache::StillImageCache;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};

const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Render once, so ffmpeg loops a small raw frame instead of decoding and scaling the full image
/// every frame. A layer that fails keeps the full-size image.
pub async fn prescale_still_images(
    pipeline: &mut Pipeline,
    cache: &StillImageCache,
    ffmpeg_path: &Path,
    client: &reqwest::Client,
) {
    for bake in pipeline.still_image_bakes() {
        match render(&bake, cache, ffmpeg_path, client).await {
            Ok(path) => pipeline.use_baked_still_image(&bake, &path.to_string_lossy()),
            Err(e) => log::warn!(
                "graphics layer {}: still image pre-scale failed, using the full-size image: {e}",
                bake.layer_index
            ),
        }
    }
}

async fn render(
    bake: &StillImageBake,
    cache: &StillImageCache,
    ffmpeg_path: &Path,
    client: &reqwest::Client,
) -> Result<PathBuf, String> {
    match &bake.input_source {
        InputSource::Local(local) => {
            let path = local
                .expand_path()
                .ok_or_else(|| format!("invalid path {}", local.path))?;
            cache
                .render(ffmpeg_path, bake, Path::new(&path))
                .await
                .map_err(|e| e.to_string())
        }
        InputSource::Http(http) => {
            let bytes = fetch(client, http).await?;
            let mut temp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
            temp.write_all(&bytes).map_err(|e| e.to_string())?;
            cache
                .render(ffmpeg_path, bake, temp.path())
                .await
                .map_err(|e| e.to_string())
        }
        InputSource::Lavfi(_) | InputSource::Rtsp(_) => {
            Err(String::from("unsupported source type"))
        }
    }
}

async fn fetch(client: &reqwest::Client, http: &HttpInputSource) -> Result<Vec<u8>, String> {
    let mut headers = HeaderMap::new();
    for header in &http.options.headers {
        let (name, value) = header
            .split_once(':')
            .ok_or_else(|| format!("invalid header {header}"))?;
        headers.insert(
            HeaderName::try_from(name.trim()).map_err(|e| e.to_string())?,
            HeaderValue::try_from(value.trim()).map_err(|e| e.to_string())?,
        );
    }
    if let Some(user_agent) = &http.options.user_agent {
        headers.insert(
            USER_AGENT,
            HeaderValue::try_from(user_agent).map_err(|e| e.to_string())?,
        );
    }
    let timeout = http
        .options
        .timeout_us
        .map_or(FETCH_TIMEOUT, Duration::from_micros);

    let response = client
        .get(&http.uri)
        .headers(headers)
        .timeout(timeout)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("fetch {}: {e}", http.uri))?;
    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("fetch {}: {e}", http.uri))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffpipeline::input::HttpInputOptions;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn fetch_sends_source_headers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let n = socket.read(&mut request).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 3\r\nconnection: close\r\n\r\npng")
                .await
                .unwrap();
            String::from_utf8_lossy(&request[..n]).to_lowercase()
        });

        let http = HttpInputSource {
            uri: format!("http://{addr}/logo.png"),
            options: HttpInputOptions {
                headers: vec![String::from("X-Token: abc")],
                user_agent: Some(String::from("etv-test")),
                ..Default::default()
            },
        };
        let bytes = fetch(&reqwest::Client::new(), &http).await.unwrap();
        assert_eq!(bytes, b"png");

        let request = server.await.unwrap();
        assert!(request.contains("x-token: abc"), "{request}");
        assert!(request.contains("user-agent: etv-test"), "{request}");
    }
}
