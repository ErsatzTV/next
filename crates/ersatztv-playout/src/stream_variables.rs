//! Expansion of stream variables in playout item source URIs.
//!
//! Stream variables share the `{{ }}` delimiters with environment templates
//! (see [`crate::template`]) and are told apart by a namespace prefix:
//!
//! - `{{NAME}}`, no namespace, is an environment variable
//! - `{{channel:number}}` is the number of the channel being transcoded
//!
//! Sharing the delimiters is safe because a name containing a colon is not a
//! valid environment variable name, so [`crate::template::expand_template`]
//! re-emits it verbatim rather than resolving or rejecting it. This module
//! runs second and claims what the first pass left behind. A test below pins
//! that contract.
//!
//! Anything that is not a known stream variable is left as written, so an
//! unresolved environment template, a future namespace, and a URI that
//! merely contains braces all pass through unchanged.

const CHANNEL_NUMBER: &str = "channel:number";

/// Expands stream variables in `input`.
///
/// Values are substituted verbatim, with no escaping of any kind. Whatever a
/// value contains reaches the remote server as written, and nothing here
/// bounds which part of the URI a substitution can affect. That is acceptable
/// only because the channel number is operator-authored, coming from the same
/// lineup as the template it lands in, so it is trusted to the same degree as
/// the text around it.
///
/// A value from a less trusted source, a request parameter for instance,
/// needs percent-encoding and a rule about which parts of the URI a
/// substitution may change. Do not resolve such a value here without adding
/// both.
pub fn expand(input: &str, channel_number: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut remaining = input;

    loop {
        let Some(open) = remaining.find("{{") else {
            result.push_str(remaining);
            return result;
        };

        result.push_str(&remaining[..open]);
        let after_open = &remaining[open + 2..];

        match after_open.find("}}") {
            Some(close) if after_open[..close].trim() == CHANNEL_NUMBER => {
                result.push_str(channel_number);
                remaining = &after_open[close + 2..];
            }
            _ => {
                // not a stream variable, so it belongs to whoever wrote it,
                // and scanning continues past the braces we just emitted
                result.push_str("{{");
                remaining = after_open;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::expand;

    #[test]
    fn expands_the_channel_number() {
        assert_eq!(
            expand(
                "http://origin.lan:8000/{{channel:number}}/stream.m3u8",
                "30"
            ),
            "http://origin.lan:8000/30/stream.m3u8"
        );
    }

    #[test]
    fn expands_every_occurrence() {
        assert_eq!(
            expand(
                "http://origin.lan:8000/{{channel:number}}/{{channel:number}}.m3u8",
                "30"
            ),
            "http://origin.lan:8000/30/30.m3u8"
        );
    }

    #[test]
    fn substitutes_the_channel_number_verbatim() {
        // nothing constrains a channel number, and it is operator-authored,
        // so it reaches the uri exactly as the lineup spells it
        assert_eq!(
            expand(
                "http://origin.lan:8000/{{channel:number}}/stream.m3u8",
                "30.1"
            ),
            "http://origin.lan:8000/30.1/stream.m3u8"
        );
    }

    #[test]
    fn tolerates_whitespace_inside_the_braces() {
        assert_eq!(
            expand(
                "http://origin.lan:8000/{{ channel:number }}/stream.m3u8",
                "30"
            ),
            "http://origin.lan:8000/30/stream.m3u8"
        );
    }

    #[test]
    fn survives_the_environment_pass() {
        // the two passes share a delimiter, so this pins the property that
        // makes that safe: a name that is not a valid environment variable
        // name is re-emitted verbatim by the first pass instead of being
        // resolved or rejected
        let after_env =
            crate::template::expand_template("http://origin.lan:8000/{{channel:number}}/s.m3u8")
                .unwrap();
        assert_eq!(expand(&after_env, "30"), "http://origin.lan:8000/30/s.m3u8");
    }

    #[test]
    fn leaves_an_environment_variable_reference_unchanged() {
        // the environment pass runs first and normally consumes these; an
        // unresolved one must not be mistaken for a stream variable
        assert_eq!(
            expand("http://origin.lan:8000/{{MY_SECRET}}/stream.m3u8", "30"),
            "http://origin.lan:8000/{{MY_SECRET}}/stream.m3u8"
        );
    }

    #[test]
    fn leaves_an_unknown_namespace_unchanged() {
        assert_eq!(
            expand("http://origin.lan:8000/{{query:region}}/stream.m3u8", "30"),
            "http://origin.lan:8000/{{query:region}}/stream.m3u8"
        );
    }

    #[test]
    fn leaves_single_braces_unchanged() {
        // single braces are not a stream variable delimiter, so a uri that
        // happens to contain them is content
        assert_eq!(
            expand("http://origin.lan:8000/{channel:number}/stream.m3u8", "30"),
            "http://origin.lan:8000/{channel:number}/stream.m3u8"
        );
    }

    #[test]
    fn leaves_an_unclosed_variable_unchanged() {
        assert_eq!(
            expand("http://origin.lan:8000/{{channel:number/stream.m3u8", "30"),
            "http://origin.lan:8000/{{channel:number/stream.m3u8"
        );
    }

    #[test]
    fn leaves_a_uri_without_variables_unchanged() {
        assert_eq!(
            expand("http://origin.lan:8000/stream.m3u8", "30"),
            "http://origin.lan:8000/stream.m3u8"
        );
    }

    #[test]
    fn expands_a_variable_that_follows_multibyte_content() {
        // the scan indexes by byte, so a multibyte character ahead of the
        // variable would break it if any offset were computed wrongly
        assert_eq!(
            expand("http://origin.lan:8000/café/{{channel:number}}.m3u8", "30"),
            "http://origin.lan:8000/café/30.m3u8"
        );
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(expand("", "30"), "");
    }
}
