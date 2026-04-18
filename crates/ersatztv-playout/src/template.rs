use crate::error::PlayoutError;

/// Expand all `{{VAR_NAME}}` occurrences in `input` using `std::env::var`.
///
/// Variable names must match `[A-Za-z_][A-Za-z0-9_]*`. Returns the expanded
/// string, or an error if any referenced variable is missing or its value
/// contains control characters (bytes < 0x20 except tab).
pub fn expand_template(input: &str) -> Result<String, PlayoutError> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        if ch == '{' {
            // Check for second '{'
            if let Some(&(_, '{')) = chars.peek() {
                chars.next(); // consume second '{'

                let name_start = i + 2; // past "{{"
                let mut name_end = name_start;
                let mut found_close = false;
                let mut partial_match = false;

                while let Some(&(j, c)) = chars.peek() {
                    if c == '}' {
                        name_end = j;
                        chars.next(); // consume first '}'
                        // Expect second '}'
                        if let Some(&(_, '}')) = chars.peek() {
                            chars.next(); // consume second '}'
                            found_close = true;
                        } else {
                            // Single '}' — not a template, emit literally
                            result.push_str(&input[i..=j]);
                            partial_match = true;
                            break;
                        }
                        break;
                    }
                    chars.next();
                }

                if partial_match {
                    continue;
                } else if found_close {
                    let var_name = input[name_start..name_end].trim();
                    if var_name.is_empty() || !is_valid_var_name(var_name) {
                        // Not a valid variable reference, emit literally
                        result.push_str("{{");
                        result.push_str(var_name);
                        result.push_str("}}");
                    } else {
                        let value = std::env::var(var_name).map_err(|_| {
                            PlayoutError::TemplateMissingEnvVar(var_name.to_string())
                        })?;

                        // Validate: no control characters except tab
                        if value.bytes().any(|b| b < 0x20 && b != b'\t') {
                            return Err(PlayoutError::TemplateInvalidEnvVarValue(
                                var_name.to_string(),
                            ));
                        }

                        result.push_str(&value);
                    }
                } else if !found_close {
                    // Reached end of input without closing "}}", emit what we consumed
                    result.push_str(&input[i..]);
                    return Ok(result);
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

fn is_valid_var_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_templates() {
        let result = expand_template("https://example.com/file.mkv").unwrap();
        assert_eq!(result, "https://example.com/file.mkv");
    }

    #[test]
    fn single_template() {
        // SAFETY: test-only, single-threaded
        unsafe { std::env::set_var("TEST_TOKEN_1", "secret123") };
        let result =
            expand_template("https://example.com/file.mkv?token={{TEST_TOKEN_1}}").unwrap();
        assert_eq!(result, "https://example.com/file.mkv?token=secret123");
    }

    #[test]
    fn consumes_extra_whitespace() {
        // SAFETY: test-only, single-threaded
        unsafe { std::env::set_var("TEST_TOKEN_1", "secret123") };
        let result =
            expand_template("https://example.com/file.mkv?token={{ TEST_TOKEN_1  }}").unwrap();
        assert_eq!(result, "https://example.com/file.mkv?token=secret123");
    }

    #[test]
    fn multiple_templates() {
        // SAFETY: test-only, single-threaded
        unsafe {
            std::env::set_var("TEST_HOST_1", "my.server.com");
            std::env::set_var("TEST_KEY_1", "abc");
        }
        let result = expand_template("https://{{TEST_HOST_1}}/path?key={{TEST_KEY_1}}").unwrap();
        assert_eq!(result, "https://my.server.com/path?key=abc");
    }

    #[test]
    fn missing_env_var() {
        // SAFETY: test-only, single-threaded
        unsafe { std::env::remove_var("TEST_NONEXISTENT_VAR") };
        let result = expand_template("{{TEST_NONEXISTENT_VAR}}");
        assert!(matches!(
            result,
            Err(PlayoutError::TemplateMissingEnvVar(_))
        ));
    }

    #[test]
    fn control_chars_rejected() {
        // SAFETY: test-only, single-threaded
        unsafe { std::env::set_var("TEST_BAD_VAR_1", "value\nwith\nnewlines") };
        let result = expand_template("{{TEST_BAD_VAR_1}}");
        assert!(matches!(
            result,
            Err(PlayoutError::TemplateInvalidEnvVarValue(_))
        ));
    }

    #[test]
    fn literal_braces_preserved() {
        let result = expand_template("single { brace } here").unwrap();
        assert_eq!(result, "single { brace } here");
    }

    #[test]
    fn malformed_single_close_brace_no_duplication() {
        // {{VAR}X should emit literally without duplication and not
        // prevent subsequent templates from being processed.
        unsafe { std::env::set_var("TEST_AFTER_MALFORMED", "ok") };
        let result = expand_template("{{VAR}X then {{TEST_AFTER_MALFORMED}}").unwrap();
        assert_eq!(result, "{{VAR}X then ok");
    }

    #[test]
    fn tab_allowed_in_value() {
        // SAFETY: test-only, single-threaded
        unsafe { std::env::set_var("TEST_TAB_VAR", "value\twith\ttabs") };
        let result = expand_template("{{TEST_TAB_VAR}}").unwrap();
        assert_eq!(result, "value\twith\ttabs");
    }
}
