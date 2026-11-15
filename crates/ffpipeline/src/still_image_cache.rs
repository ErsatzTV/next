use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};

use crate::pipeline::{Sampling, StillImageBake};

const RENDER_TIMEOUT: Duration = Duration::from_secs(30);
// an older temp file is from a render that died
const ABANDONED_TEMP_AGE: Duration = Duration::from_hours(1);
const RAW_EXTENSION: &str = "raw";
const TEMP_SUFFIX: &str = ".tmp";
// cleanup deletes by age, so it must never see files it didn't write
const SUBFOLDER: &str = "still-images";

#[derive(thiserror::Error, Debug)]
pub enum StillImageCacheError {
    #[error("{context}: {source}")]
    Io {
        context: String,
        source: std::io::Error,
    },
    #[error("ffmpeg render failed ({status}): {stderr}")]
    Render { status: String, stderr: String },
    #[error("ffmpeg render timed out after {0:?}")]
    Timeout(Duration),
}

fn io_error(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> StillImageCacheError {
    move |source| StillImageCacheError::Io {
        context: context.into(),
        source,
    }
}

/// Raw frames loop without decoding. Keyed by source content and render settings, so channels
/// share them.
#[derive(Debug, Clone)]
pub struct StillImageCache {
    folder: PathBuf,
    max_unused: Duration,
}

impl StillImageCache {
    pub fn new(cache_folder: &Path, max_unused: Duration) -> StillImageCache {
        StillImageCache {
            folder: cache_folder.join(SUBFOLDER),
            max_unused,
        }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub async fn render(
        &self,
        ffmpeg_path: &Path,
        bake: &StillImageBake,
        source: &Path,
    ) -> Result<PathBuf, StillImageCacheError> {
        let bytes = tokio::fs::read(source)
            .await
            .map_err(io_error(format!("read {}", source.display())))?;
        let path = self.folder.join(file_name(&bytes, bake));

        if touch(&path).is_ok() {
            log::debug!("still image cache hit {}", path.display());
            return Ok(path);
        }

        tokio::fs::create_dir_all(&self.folder)
            .await
            .map_err(io_error(format!("create {}", self.folder.display())))?;
        self.sweep().await;

        // one temp file per render, so concurrent renders of one image don't collide
        let temp = tempfile::Builder::new()
            .prefix(".")
            .suffix(TEMP_SUFFIX)
            .tempfile_in(&self.folder)
            .map_err(io_error(format!(
                "create temp file in {}",
                self.folder.display()
            )))?
            .into_temp_path();

        let mut command = ersatztv_core::process::command(ffmpeg_path);
        command
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(source)
            .args([
                "-map".to_owned(),
                format!("0:{}", bake.stream_index),
                "-frames:v".to_owned(),
                "1".to_owned(),
                "-vf".to_owned(),
                bake.filter.clone(),
                "-pix_fmt".to_owned(),
                bake.pixel_format.as_arg().to_owned(),
            ])
            // raw has no range tag; the graph converts a full-range (jpeg) frame to tv before the
            // overlay, so do the same here
            .args(
                (bake.pixel_format.sampling() != Sampling::Rgb)
                    .then_some(["-color_range", "tv"])
                    .into_iter()
                    .flatten(),
            )
            .args(["-f", "rawvideo"])
            .arg(&temp)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let output = tokio::time::timeout(RENDER_TIMEOUT, command.output())
            .await
            .map_err(|_| StillImageCacheError::Timeout(RENDER_TIMEOUT))?
            .map_err(io_error(format!("run {}", ffmpeg_path.display())))?;

        let rendered_len = std::fs::metadata(&temp).map(|m| m.len()).unwrap_or(0);
        if !output.status.success() || rendered_len == 0 {
            return Err(StillImageCacheError::Render {
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }

        if let Err(e) = temp.persist(&path) {
            // on Windows the rename fails while another channel's ffmpeg reads the same render
            if !path.exists() {
                return Err(StillImageCacheError::Io {
                    context: format!("rename to {}", path.display()),
                    source: e.error,
                });
            }
        }
        log::debug!("rendered still image {}", path.display());
        Ok(path)
    }

    async fn sweep(&self) {
        let Ok(mut entries) = tokio::fs::read_dir(&self.folder).await else {
            return;
        };
        let now = SystemTime::now();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let max_age = if path.extension().is_some_and(|e| e == RAW_EXTENSION) {
                self.max_unused
            } else if name.starts_with('.') && name.ends_with(TEMP_SUFFIX) {
                ABANDONED_TEMP_AGE
            } else {
                continue;
            };
            let unused_for = entry
                .metadata()
                .await
                .and_then(|m| m.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok());
            if unused_for.is_some_and(|age| age > max_age) {
                match tokio::fs::remove_file(&path).await {
                    Ok(()) => log::debug!("removed unused still image {}", path.display()),
                    Err(e) => log::debug!("failed to remove {}: {e}", path.display()),
                }
            }
        }
    }
}

// bump when the render command changes what a key produces
const KEY_VERSION: u8 = 1;

fn file_name(source: &[u8], bake: &StillImageBake) -> String {
    let mut hasher = DefaultHasher::new();
    KEY_VERSION.hash(&mut hasher);
    source.hash(&mut hasher);
    bake.stream_index.hash(&mut hasher);
    bake.filter.hash(&mut hasher);
    bake.pixel_format.as_arg().hash(&mut hasher);
    (bake.size.width, bake.size.height).hash(&mut hasher);
    format!(
        "{:016x}_{}x{}_{}.{RAW_EXTENSION}",
        hasher.finish(),
        bake.size.width,
        bake.size.height,
        bake.pixel_format.as_arg()
    )
}

/// mtime records last use for [`StillImageCache::sweep`].
fn touch(path: &Path) -> std::io::Result<()> {
    std::fs::File::options()
        .write(true)
        .open(path)?
        .set_modified(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_size::FrameSize;
    use crate::input::{InputSource, LocalInputSource};
    use crate::pipeline::PixelFormat;

    fn bake(filter: &str) -> StillImageBake {
        StillImageBake {
            layer_index: 0,
            input_source: InputSource::Local(LocalInputSource {
                path: String::from("logo.png"),
            }),
            stream_index: 0,
            filter: filter.to_owned(),
            pixel_format: PixelFormat::Bgra,
            size: FrameSize {
                width: 192,
                height: 108,
            },
        }
    }

    #[test]
    fn file_name_changes_with_content_and_settings() {
        let a = file_name(b"one", &bake("scale=192:108"));
        assert!(a.ends_with("_192x108_bgra.raw"), "{a}");
        assert_eq!(a, file_name(b"one", &bake("scale=192:108")));
        assert_ne!(a, file_name(b"two", &bake("scale=192:108")));
        assert_ne!(
            a,
            file_name(b"one", &bake("colorchannelmixer=aa=0.5,scale=192:108"))
        );
    }

    #[tokio::test]
    async fn hit_touches_and_sweep_removes_only_unused_frames() {
        let dir = tempfile::tempdir().unwrap();
        let cache = StillImageCache::new(dir.path(), Duration::from_hours(24));
        let source = dir.path().join("logo.png");
        std::fs::write(&source, b"png").unwrap();

        let old = SystemTime::now() - Duration::from_hours(48);
        std::fs::create_dir(cache.folder()).unwrap();
        let hit = cache.folder().join(file_name(b"png", &bake("null")));
        let stale = cache.folder().join("0000000000000000_1x1_bgra.raw");
        let abandoned = cache.folder().join(".abc.tmp");
        let unrelated_raw = dir.path().join("other.raw");
        let unrelated_temp = dir.path().join(".other.tmp");
        for path in [&hit, &stale, &abandoned, &unrelated_raw, &unrelated_temp] {
            std::fs::write(path, b"x").unwrap();
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(old)
                .unwrap();
        }

        // a hit never runs ffmpeg
        let path = cache
            .render(Path::new("/nonexistent/ffmpeg"), &bake("null"), &source)
            .await
            .unwrap();
        assert_eq!(path, hit);
        let modified = std::fs::metadata(&hit).unwrap().modified().unwrap();
        assert!(modified > old + Duration::from_hours(47));

        cache.sweep().await;
        assert!(hit.exists());
        assert!(!stale.exists());
        assert!(!abandoned.exists());
        assert!(source.exists());
        assert!(unrelated_raw.exists());
        assert!(unrelated_temp.exists());
    }
}
