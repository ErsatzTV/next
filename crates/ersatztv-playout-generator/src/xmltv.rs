use std::io::{Cursor, ErrorKind};
use std::path::Path;

use ersatztv_playout::playout::{PlayoutItem, PlayoutItemSource};
use quick_xml::events::{BytesDecl, BytesText, Event};
use quick_xml::{Reader, Writer, XmlVersion};
use time::OffsetDateTime;
use time::format_description::StaticFormatDescription;
use time::macros::format_description;

const XMLTV_FMT: StaticFormatDescription = format_description!(
    "[year][month][day][hour][minute][second] [offset_hour sign:mandatory][offset_minute]"
);

use crate::error::PlayoutGeneratorError;

pub(crate) struct XmltvProgramme {
    pub start: OffsetDateTime,
    pub stop: OffsetDateTime,
    pub title: String,
}

impl From<&PlayoutItem> for XmltvProgramme {
    fn from(item: &PlayoutItem) -> Self {
        Self {
            start: item.start,
            stop: item.finish,
            title: parse_title(item),
        }
    }
}

pub(crate) async fn write_xmltv_file(
    xmltv_folder: &Path,
    channel_tvg_id: &str,
    playout_items: &[XmltvProgramme],
    retain_after: OffsetDateTime,
) -> Result<(), PlayoutGeneratorError> {
    let output_file = xmltv_folder.join(format!("{}.xml", channel_tvg_id));

    let kept = match tokio::fs::read(&output_file).await {
        Ok(bytes) => {
            read_existing_programmes(&bytes, retain_after, playout_items.first().map(|i| i.start))?
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };

    let mut writer = Writer::new(Cursor::new(Vec::new()));
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
    writer.create_element("tv").write_inner_content(|writer| {
        for playout_item in kept.iter().chain(playout_items) {
            let start = playout_item
                .start
                .format(&XMLTV_FMT)
                .map_err(std::io::Error::other)?;
            let stop = playout_item
                .stop
                .format(&XMLTV_FMT)
                .map_err(std::io::Error::other)?;

            writer
                .create_element("programme")
                .with_attribute(("channel", channel_tvg_id))
                .with_attribute(("start", start.as_str()))
                .with_attribute(("stop", stop.as_str()))
                .write_inner_content(|writer| {
                    writer
                        .create_element("title")
                        .write_text_content(BytesText::new(playout_item.title.as_str()))?;

                    Ok(())
                })?;
        }

        Ok(())
    })?;

    tokio::fs::write(output_file, writer.into_inner().into_inner()).await?;

    Ok(())
}

fn read_existing_programmes(
    bytes: &[u8],
    retain_after: OffsetDateTime,
    new_start: Option<OffsetDateTime>,
) -> Result<Vec<XmltvProgramme>, PlayoutGeneratorError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut inner_buf = Vec::new();
    let mut out = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(std::io::Error::other)?
        {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == b"programme" => {
                let (mut start_s, mut stop_s) = (None, None);
                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"start" => {
                            start_s = Some(
                                attr.normalized_value(XmlVersion::Implicit1_0)
                                    .map_err(std::io::Error::other)?
                                    .into_owned(),
                            )
                        }
                        b"stop" => {
                            stop_s = Some(
                                attr.normalized_value(XmlVersion::Implicit1_0)
                                    .map_err(std::io::Error::other)?
                                    .into_owned(),
                            )
                        }
                        _ => {}
                    }
                }
                let (Some(start_s), Some(stop_s)) = (start_s, stop_s) else {
                    reader
                        .read_to_end_into(e.to_end().name(), &mut inner_buf)
                        .map_err(std::io::Error::other)?;
                    continue;
                };
                let start = OffsetDateTime::parse(&start_s, &XMLTV_FMT)?;
                let stop = OffsetDateTime::parse(&stop_s, &XMLTV_FMT)?;

                let mut title = String::new();
                loop {
                    match reader
                        .read_event_into(&mut inner_buf)
                        .map_err(std::io::Error::other)?
                    {
                        Event::Start(t) if t.name().as_ref() == b"title" => {
                            let mut text_buf = Vec::new();
                            let text_event = reader
                                .read_text_into(t.name(), &mut text_buf)
                                .map_err(std::io::Error::other)?;
                            let text = text_event.decode().map_err(std::io::Error::other)?;
                            title = quick_xml::escape::unescape(&text)
                                .map_err(std::io::Error::other)?
                                .into_owned();
                        }
                        Event::End(t) if t.name().as_ref() == b"programme" => break,
                        Event::Eof => break,
                        _ => {}
                    }
                    inner_buf.clear();
                }

                if stop > retain_after && new_start.is_none_or(|s| start < s) {
                    out.push(XmltvProgramme { start, stop, title });
                }
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(out)
}

fn parse_title(playout_item: &PlayoutItem) -> String {
    match &playout_item.source {
        Some(PlayoutItemSource::Local { path, .. }) => Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone()),
        _ => playout_item.id.clone(),
    }
}
