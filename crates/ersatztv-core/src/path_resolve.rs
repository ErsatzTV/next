use std::path::Path;

use serde_json::Value;
use simple_expand_tilde::expand_tilde;

pub fn resolve_relative_paths(value: &mut Value, base_dir: &Path, pointers: &[&str]) {
    for pointer in pointers {
        if let Some(Value::String(s)) = value.pointer_mut(pointer)
            && !s.is_empty()
        {
            let expanded = expand_tilde(&s).unwrap_or(Path::new(s).to_path_buf());

            *s = if expanded.is_relative() {
                base_dir
                    .join(&expanded)
                    .canonicalize()
                    .unwrap_or_else(|_| base_dir.join(&expanded))
                    .to_string_lossy()
                    .to_string()
            } else {
                expanded.to_string_lossy().to_string()
            };
        }
    }
}
