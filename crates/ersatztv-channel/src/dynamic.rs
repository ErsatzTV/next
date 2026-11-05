//! Part of the playout schema contract, pinned by `tests/dynamic_contract.rs`. Read AGENTS.md
//! "Dynamic source callback" before changing it.

use ersatztv_playout::playout::{PlayoutItem, PlayoutItemSource};
use ersatztv_playout::template::expand_template;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::dynamic_retry::{RetryPolicy, get_text, with_retry};
use crate::error::ChannelError;

pub const DYNAMIC_ID_HEADER: &str = "x-etv-dynamic-id";
pub const CHANNEL_HEADER: &str = "x-etv-channel";
pub const NOW_HEADER: &str = "x-etv-now";
pub const UNTIL_HEADER: &str = "x-etv-until";
pub const PLAYOUT_SCHEMA_HEADER: &str = "x-etv-playout-schema";

pub const CALLBACK_HEADERS: [&str; 5] = [
    DYNAMIC_ID_HEADER,
    CHANNEL_HEADER,
    NOW_HEADER,
    UNTIL_HEADER,
    PLAYOUT_SCHEMA_HEADER,
];

pub async fn resolve(
    client: &reqwest::Client,
    channel_number: &str,
    start: &OffsetDateTime,
    dynamic_item: &PlayoutItem,
) -> Result<PlayoutItem, ChannelError> {
    let Some(PlayoutItemSource::Dynamic {
        uri,
        headers,
        user_agent,
        timeout_us,
        retry_budget_ms,
        retry_backoff_ms,
    }) = &dynamic_item.source
    else {
        return Err(ChannelError::DynamicSourceRequired);
    };

    let expanded_uri = expand_template(uri)?;
    let expanded_headers: Vec<String> = headers
        .iter()
        .flatten()
        .map(|h| expand_template(h))
        .collect::<Result<Vec<_>, _>>()?;
    let expanded_ua = user_agent.as_deref().map(expand_template).transpose()?;

    let mut header_map = HeaderMap::new();
    for h in &expanded_headers {
        let Some((name, value)) = h.split_once(':') else {
            continue;
        };
        let name = HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|e| ChannelError::DynamicSourceFailure(format!("bad header name: {e}")))?;
        let value = HeaderValue::from_str(value.trim())
            .map_err(|e| ChannelError::DynamicSourceFailure(format!("bad header value: {e}")))?;
        header_map.insert(name, value);
    }
    if let Some(ua) = expanded_ua.as_deref() {
        header_map.insert(
            USER_AGENT,
            HeaderValue::from_str(ua)
                .map_err(|e| ChannelError::DynamicSourceFailure(format!("bad user agent: {e}")))?,
        );
    }

    let mut insert = |name: &'static str, value: &str, what: &str| {
        let value = HeaderValue::from_str(value)
            .map_err(|e| ChannelError::DynamicSourceFailure(format!("bad {what}: {e}")))?;
        header_map.insert(HeaderName::from_static(name), value);
        Ok::<_, ChannelError>(())
    };
    insert(DYNAMIC_ID_HEADER, &dynamic_item.id, "dynamic source id")?;
    insert(CHANNEL_HEADER, channel_number, "channel number")?;
    insert(NOW_HEADER, &start.format(&Rfc3339)?, "time value")?;
    insert(
        UNTIL_HEADER,
        &dynamic_item.finish.format(&Rfc3339)?,
        "time value",
    )?;
    insert(
        PLAYOUT_SCHEMA_HEADER,
        &ersatztv_playout::playout::SCHEMA.uri(),
        "playout schema",
    )?;

    let policy = RetryPolicy::new(*timeout_us, *retry_budget_ms, *retry_backoff_ms);
    let body = with_retry(&policy, |timeout| {
        get_text(client, &expanded_uri, &header_map, timeout)
    })
    .await
    .map_err(ChannelError::DynamicSourceFailure)?;

    let (mut item, issue) = ersatztv_playout::playout::item_from_str(&body)
        .map_err(|e| ChannelError::DynamicSourceFailure(format!("invalid playout item: {e}")))?;
    if let Some(issue) = issue {
        log::warn!(
            "dynamic source {} returned item {} with an invalid probe hint, playing without hints: {}",
            dynamic_item.id,
            item.id,
            issue.error
        );
    }

    if item.start != *start {
        let duration = item.finish - item.start;
        item.start = *start;
        item.finish = *start + duration;
    }

    if item.finish > dynamic_item.finish {
        item.finish = dynamic_item.finish;
    }

    if item.finish <= item.start {
        return Err(ChannelError::DynamicSourceNoRemainingTime);
    }

    if has_dynamic_source(&item) {
        return Err(ChannelError::DynamicSourceCannotRecurse);
    }

    Ok(item)
}

fn has_dynamic_source(item: &PlayoutItem) -> bool {
    let is_dynamic = |source: Option<&PlayoutItemSource>| {
        matches!(source, Some(PlayoutItemSource::Dynamic { .. }))
    };

    let tracks = item.tracks.as_ref();
    is_dynamic(item.source.as_ref())
        || [
            tracks.and_then(|t| t.audio.as_ref()),
            tracks.and_then(|t| t.video.as_ref()),
            tracks.and_then(|t| t.subtitle.as_ref()),
        ]
        .into_iter()
        .flatten()
        .any(|track| is_dynamic(track.source.as_ref()))
        || item
            .watermark
            .as_ref()
            .is_some_and(|w| is_dynamic(Some(&w.source)))
        || item
            .graphics
            .iter()
            .any(|layer| is_dynamic(Some(&layer.source)))
}
