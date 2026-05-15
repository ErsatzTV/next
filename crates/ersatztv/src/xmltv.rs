use std::io::{BufRead, BufReader, Cursor, ErrorKind, Write};
use std::path::Path;

use ersatztv::error::LineupError;
use quick_xml::Reader;
use quick_xml::events::{BytesDecl, BytesText, Event};
use quick_xml::writer::Writer;

use crate::LineupState;
use crate::channel_model::ChannelModel;

struct ChannelMeta {
    pub name: String,
    pub tvg_id: String,
    pub logo: Option<String>,
}

impl From<&ChannelModel> for ChannelMeta {
    fn from(model: &ChannelModel) -> Self {
        Self {
            name: model.name().to_string(),
            tvg_id: model.tvg_id().to_string(),
            logo: model.logo().map(|s| s.to_string()),
        }
    }
}

pub async fn generate(state: &LineupState) -> Result<Vec<u8>, LineupError> {
    let folder = state.xmltv_folder.clone();
    let channels: Vec<ChannelMeta> = state.channels.iter().map(ChannelMeta::from).collect();

    tokio::task::spawn_blocking(move || generate_blocking(folder.as_deref(), &channels)).await?
}

fn generate_blocking(
    folder: Option<&str>,
    channels: &[ChannelMeta],
) -> Result<Vec<u8>, LineupError> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
    writer
        .create_element("tv")
        .with_attribute(("generator-info-name", "ErsatzTV"))
        .write_inner_content(|w| {
            for channel in channels {
                w.create_element("channel")
                    .with_attribute(("id", channel.tvg_id.as_str()))
                    .write_inner_content(|inner_writer| {
                        inner_writer
                            .create_element("display-name")
                            .write_text_content(BytesText::new(channel.name.as_str()))?;

                        if let Some(logo) = &channel.logo {
                            inner_writer
                                .create_element("icon")
                                .with_attribute(("src", logo.as_str()))
                                .write_empty()?;
                        }

                        Ok(())
                    })?;
            }

            if let Some(folder) = folder {
                let folder_path = Path::new(folder);
                let mut buf = Vec::with_capacity(8 * 1024);
                for channel in channels {
                    let path = folder_path.join(format!("{}.xml", channel.tvg_id));
                    let file = match std::fs::File::open(&path) {
                        Ok(f) => f,
                        Err(e) if e.kind() == ErrorKind::NotFound => continue,
                        Err(e) => return Err(e),
                    };

                    copy_programmes(BufReader::new(file), w, &mut buf)?;
                }
            }

            Ok(())
        })?;

    Ok(writer.into_inner().into_inner())
}

fn copy_programmes<R: BufRead, W: Write>(
    reader: R,
    writer: &mut Writer<W>,
    buf: &mut Vec<u8>,
) -> std::io::Result<()> {
    let mut reader = Reader::from_reader(reader);
    reader.config_mut().trim_text(true);
    let mut depth = 0u32;
    loop {
        match reader.read_event_into(buf).map_err(std::io::Error::other)? {
            Event::Start(e) if e.name().as_ref() == b"programme" || depth > 0 => {
                depth += 1;
                writer.write_event(Event::Start(e))?;
            }
            Event::End(e) if depth > 0 => {
                depth -= 1;
                writer.write_event(Event::End(e))?;
            }
            Event::Empty(e) if e.name().as_ref() == b"programme" || depth > 0 => {
                writer.write_event(Event::Empty(e))?
            }
            Event::Text(e) if depth > 0 => writer.write_event(Event::Text(e))?,
            Event::CData(e) if depth > 0 => writer.write_event(Event::CData(e))?,
            Event::Eof => return Ok(()),
            _ => {}
        }

        buf.clear();
    }
}
