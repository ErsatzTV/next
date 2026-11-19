use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// own subfolder, since sweep deletes by age
const LIBPLACEBO_SUBFOLDER: &str = "libplacebo";

pub async fn prepare_libplacebo(cache_folder: &Path, max_age: Duration) -> Option<PathBuf> {
    let folder = cache_folder.join(LIBPLACEBO_SUBFOLDER);
    if let Err(e) = tokio::fs::create_dir_all(&folder).await {
        log::warn!(
            "libplacebo shader cache disabled; failed to create {}: {e}",
            folder.display()
        );
        return None;
    }

    tokio::spawn(sweep(folder.clone(), max_age));
    Some(folder)
}

const DRIVER_CACHE_VARIABLES: &[&str] = &["XDG_CACHE_HOME", "CUDA_CACHE_PATH"];

/// Drivers only create their own subfolder in these; docker points them into an empty volume.
pub async fn create_driver_cache_folders() {
    for variable in DRIVER_CACHE_VARIABLES {
        let Some(folder) = std::env::var_os(variable).filter(|f| !f.is_empty()) else {
            continue;
        };
        if let Err(e) = tokio::fs::create_dir_all(&folder).await {
            log::warn!(
                "failed to create {variable} folder {}: {e}",
                Path::new(&folder).display()
            );
        }
    }
}

/// libplacebo names entries by their 16 hex digit key
fn is_libplacebo_entry(name: &str) -> bool {
    name.len() == 16 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// libplacebo doesn't touch entries it reads, so age is time since written: an entry in use is
/// recompiled once per `max_age`, and entries for old drivers don't pile up.
async fn sweep(folder: PathBuf, max_age: Duration) {
    let Ok(mut entries) = tokio::fs::read_dir(folder).await else {
        return;
    };
    let now = SystemTime::now();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if !is_libplacebo_entry(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let age = entry
            .metadata()
            .await
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok());
        if age.is_some_and(|age| age > max_age) {
            let path = entry.path();
            match tokio::fs::remove_file(&path).await {
                Ok(()) => log::debug!("removed old libplacebo shader {}", path.display()),
                Err(e) => log::debug!("failed to remove {}: {e}", path.display()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn age(path: &Path, age: Duration) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - age)
            .unwrap();
    }

    #[tokio::test]
    async fn sweep_removes_only_old_entries() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(LIBPLACEBO_SUBFOLDER);
        std::fs::create_dir(&folder).unwrap();

        let old = folder.join("39a3a1dce3a1a0cf");
        let fresh = folder.join("07262ed4505ae237");
        let foreign = folder.join("notes.txt");
        for path in [&old, &fresh, &foreign] {
            std::fs::write(path, b"x").unwrap();
        }
        age(&old, Duration::from_hours(48));
        age(&foreign, Duration::from_hours(48));

        sweep(folder, Duration::from_hours(24)).await;

        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(foreign.exists());
    }

    #[tokio::test]
    async fn prepare_creates_missing_folder() {
        let dir = tempfile::tempdir().unwrap();
        let cache_folder = dir.path().join("cache");

        let prepared = prepare_libplacebo(&cache_folder, Duration::from_hours(24)).await;

        let expected = cache_folder.join(LIBPLACEBO_SUBFOLDER);
        assert!(expected.is_dir());
        assert_eq!(prepared, Some(expected));
    }

    #[tokio::test]
    async fn prepare_disables_cache_when_folder_creation_fails() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LIBPLACEBO_SUBFOLDER), b"x").unwrap();

        assert_eq!(
            prepare_libplacebo(dir.path(), Duration::from_hours(24)).await,
            None
        );
    }
}
