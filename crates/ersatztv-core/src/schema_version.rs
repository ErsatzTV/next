use serde_json::Value;
use thiserror::Error;

// TODO: support major version post-1.0
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaVersion {
    pub breaking: u32,
    pub compatible: u32,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SchemaVersionError {
    #[error("unrecognized schema version '{0}'")]
    Unrecognized(String),

    #[error("found unsupported schema version {found}, expected {supported}")]
    Unsupported { found: String, supported: String },

    #[error("missing schema version (read as 0.0.0) is no longer supported, expected {supported}")]
    MissingUnsupported { supported: String },
}

/// `prefix` excludes the `0.` major version.
#[derive(Debug, Clone, Copy)]
pub struct VersionedSchema {
    pub prefix: &'static str,
    pub supported: SchemaVersion,
}

impl VersionedSchema {
    pub const fn new(prefix: &'static str, supported: SchemaVersion) -> Self {
        VersionedSchema { prefix, supported }
    }

    pub fn uri(&self) -> String {
        format!(
            "{}0.{}.{}",
            self.prefix, self.supported.breaking, self.supported.compatible
        )
    }

    pub fn parse(&self, uri: &str) -> Option<SchemaVersion> {
        let rest = uri.strip_prefix(self.prefix)?.strip_prefix("0.")?;
        let (b, c) = rest.split_once('.')?;
        Some(SchemaVersion {
            breaking: b.parse().ok()?,
            compatible: c.parse().ok()?,
        })
    }

    pub fn check(&self, uri: &str) -> Result<SchemaVersion, SchemaVersionError> {
        let found = self
            .parse(uri)
            .ok_or_else(|| SchemaVersionError::Unrecognized(uri.to_string()))?;

        if !self.supports(found) {
            return Err(SchemaVersionError::Unsupported {
                found: uri.to_string(),
                supported: self.uri(),
            });
        }

        Ok(found)
    }

    /// Missing = 0.0.0, i.e. written before the schema had a version.
    pub fn check_optional(&self, uri: Option<&str>) -> Result<SchemaVersion, SchemaVersionError> {
        let Some(uri) = uri else {
            let found = SchemaVersion {
                breaking: 0,
                compatible: 0,
            };
            return if self.supports(found) {
                Ok(found)
            } else {
                Err(SchemaVersionError::MissingUnsupported {
                    supported: self.uri(),
                })
            };
        };

        self.check(uri)
    }

    pub fn take_and_check(
        &self,
        document: &mut Value,
    ) -> Result<SchemaVersion, SchemaVersionError> {
        match document.as_object_mut().and_then(|o| o.remove("version")) {
            None => self.check_optional(None),
            Some(Value::String(uri)) => self.check(&uri),
            Some(other) => Err(SchemaVersionError::Unrecognized(other.to_string())),
        }
    }

    fn supports(&self, found: SchemaVersion) -> bool {
        found.breaking == self.supported.breaking && found.compatible <= self.supported.compatible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: VersionedSchema = VersionedSchema::new(
        "https://ersatztv.org/test/version/",
        SchemaVersion {
            breaking: 1,
            compatible: 2,
        },
    );

    fn uri(version: &str) -> String {
        format!("https://ersatztv.org/test/version/{version}")
    }

    #[test]
    fn uri_round_trips() {
        assert_eq!(SCHEMA.uri(), uri("0.1.2"));
        assert_eq!(SCHEMA.check(&SCHEMA.uri()), Ok(SCHEMA.supported));
    }

    #[test]
    fn older_compatible_is_accepted() {
        assert_eq!(
            SCHEMA.check(&uri("0.1.0")),
            Ok(SchemaVersion {
                breaking: 1,
                compatible: 0
            })
        );
    }

    #[test]
    fn newer_compatible_or_other_breaking_is_unsupported() {
        for version in ["0.1.3", "0.0.2", "0.2.0"] {
            assert_eq!(
                SCHEMA.check(&uri(version)),
                Err(SchemaVersionError::Unsupported {
                    found: uri(version),
                    supported: uri("0.1.2"),
                }),
                "{version}"
            );
        }
    }

    #[test]
    fn missing_reads_as_000() {
        let unversioned = VersionedSchema::new(
            SCHEMA.prefix,
            SchemaVersion {
                breaking: 0,
                compatible: 1,
            },
        );
        assert_eq!(
            unversioned.check_optional(None),
            Ok(SchemaVersion {
                breaking: 0,
                compatible: 0
            })
        );
        assert_eq!(
            SCHEMA.check_optional(None),
            Err(SchemaVersionError::MissingUnsupported {
                supported: uri("0.1.2"),
            })
        );
        assert_eq!(
            SCHEMA.check_optional(Some(&uri("0.1.1"))),
            SCHEMA.check(&uri("0.1.1"))
        );
    }

    #[test]
    fn take_and_check_removes_version() {
        let mut document = serde_json::json!({ "version": uri("0.1.0"), "other": 1 });
        assert!(SCHEMA.take_and_check(&mut document).is_ok());
        assert_eq!(document, serde_json::json!({ "other": 1 }));

        let mut document = serde_json::json!({ "version": 1 });
        assert_eq!(
            SCHEMA.take_and_check(&mut document),
            Err(SchemaVersionError::Unrecognized(String::from("1")))
        );

        let mut document = serde_json::json!({ "other": 1 });
        assert_eq!(
            SCHEMA.take_and_check(&mut document),
            Err(SchemaVersionError::MissingUnsupported {
                supported: uri("0.1.2"),
            })
        );
    }

    #[test]
    fn malformed_is_unrecognized() {
        for bad in [
            "https://ersatztv.org/playout/version/0.1.2".to_string(),
            uri("1.1.2"),
            uri("0.1"),
            uri("0.1.x"),
            uri("0.1.2.3"),
            String::new(),
        ] {
            assert_eq!(
                SCHEMA.check(&bad),
                Err(SchemaVersionError::Unrecognized(bad.clone())),
                "{bad}"
            );
        }
    }
}
