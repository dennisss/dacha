mod syntax;
mod template;

use core::ops::Deref;
use std::time::Duration;

use common::{errors::*, list::Appendable};
use http::uri::Uri;
use reflection::ParseFrom;

use crate::{
    mpd::template::{SegmentTemplateInputs, SegmentTemplateStr},
    CENCContentProtection, Codec, Container, ContentProtection, MediaSource, MediaTrack,
    MediaTrackKind, MediaTrackMetadata, MediaTrackSegment, WidevineContentProtection,
};

use self::syntax::*;

pub const W3C_COMMON_PSSH_BOX_ID_URI: &'static str =
    "urn:uuid:1077efec-c0b2-4d02-ace3-3c1e52e2fb4b";
pub const WIDEVINE_ID_URI: &'static str = "urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed";

pub fn read_mpd(data: &[u8], document_uri: Option<http::uri::Uri>) -> Result<MediaSource> {
    let mpd = {
        let data = xml::parse(std::str::from_utf8(data)?)?;
        syntax::MPD::parse_from(xml::ElementParser::new(&data.root_element, true))?
    };

    let mut tracks = vec![];

    println!("Duration: {:?}", mpd.mediaPresentationDuration);

    let mut context = MPDContext {
        base_urls: vec![],
        segment_base: None,
        segment_list: None,
        segment_template: None,
        repr_base: RepresentationBaseAttributes::default(),
    };

    if let Some(uri) = document_uri {
        context.base_urls.push(BaseURL {
            value: uri.to_string()?,
            byteRange: None,
        });
    }

    context.merge_base_urls(&mpd.children.base_url);

    if mpd.children.periods.len() != 1 {
        return Err(err_msg("More than one period is not supported"));
    }

    let mut last_period_end_time = Duration::from_secs(0);

    for (period_i, period) in mpd.children.periods.iter().enumerate() {
        println!("Period {:?}:", period.id);

        let period_start_time = {
            if let Some(t) = period.start.clone() {
                t.to_std_duration().ok_or_else(|| err_msg("Bad duration"))?
            } else {
                last_period_end_time.clone()
            }
        };

        let period_duration = {
            if let Some(v) = period.duration.clone() {
                v.to_std_duration().ok_or_else(|| err_msg("Bad duration"))?
            } else if period_i + 1 < mpd.children.periods.len()
                && mpd.children.periods[period_i + 1].start.is_some()
            {
                let next_start = mpd.children.periods[period_i + 1]
                    .start
                    .clone()
                    .unwrap()
                    .to_std_duration()
                    .ok_or_else(|| err_msg("Bad duration"))?;

                next_start
                    .checked_sub(period_start_time)
                    .ok_or_else(|| err_msg("Negative time"))?
            } else if period_i + 1 == mpd.children.periods.len()
                && mpd.mediaPresentationDuration.is_some()
            {
                let end = mpd
                    .mediaPresentationDuration
                    .clone()
                    .unwrap()
                    .to_std_duration()
                    .ok_or_else(|| err_msg("Bad duration"))?;

                end.checked_sub(period_start_time)
                    .ok_or_else(|| err_msg("Negative time"))?
            } else {
                return Err(err_msg("Can't determine the duration of the period"));
            }
        };

        last_period_end_time = period_start_time + period_duration;

        let mut context = context.clone();
        context.merge_base_urls(&period.children.base_url)?;
        context.merge_segment_base(&period.children.segment_base)?;
        context.merge_segment_list(&period.children.segment_list)?;
        context.merge_segment_template(&period.children.segment_template)?;

        for adaptation_set in &period.children.adaptation_sets[..] {
            let mut context = context.clone();
            context.merge_base_urls(&adaptation_set.children.base_url)?;
            context.merge_segment_base(&adaptation_set.children.segment_base)?;
            context.merge_segment_list(&adaptation_set.children.segment_list)?;
            context.merge_segment_template(&adaptation_set.children.segment_template)?;
            context.merge_repr_base(&adaptation_set.base);

            // adaptation_set.

            // <Role schemeIdUri="urn:mpeg:dash:role:2011" value="main"/>

            // TODO: Check the codecs both at the AdaptationSet and Representation level.

            // ContentType

            println!(
                "  AdaptationSet: [lang: {lang:?}] [contentType: {ct:?}] [mimeType: {mt:?}]",
                lang = adaptation_set.lang,
                ct = adaptation_set.contentType,
                mt = adaptation_set.base.mimeType
            );

            if adaptation_set.children.representations.is_empty() {
                return Err(err_msg("No representations for AdaptationSet"));
            }

            for repr in &adaptation_set.children.representations[..] {
                let mut context = context.clone();
                context.merge_base_urls(&repr.children.base_url)?;
                context.merge_segment_base(&repr.children.segment_base)?;
                context.merge_segment_list(&repr.children.segment_list)?;
                context.merge_segment_template(&repr.children.segment_template)?;
                context.merge_repr_base(&repr.base);

                if !repr.children.sub_representations.is_empty() {
                    return Err(err_msg("Sub representations not supported"));
                }

                // Done merging. Start resolving stuff.

                let content_type = &adaptation_set.contentType;
                let mime_type = &context.repr_base.mimeType;

                let mut kind = MediaTrackKind::Unknown;
                let mut container = None;
                let mut codec = None;

                match content_type.as_ref().map(|s| s.as_str()) {
                    Some("text") => kind = MediaTrackKind::Subtitle,
                    Some("audio") => kind = MediaTrackKind::Audio,
                    Some("video") => kind = MediaTrackKind::Video,
                    _ => {}
                };

                if let Some(s) = mime_type {
                    if s.starts_with("video/") {
                        kind = MediaTrackKind::Video;
                    } else if s.starts_with("audio/") {
                        kind = MediaTrackKind::Audio;
                    }

                    if s.ends_with("/mp4") {
                        container = Some(Container::MP4);
                    } else if s.ends_with("/webm") {
                        container = Some(Container::WebM);
                    }
                }

                if let Some(c) = &context.repr_base.codecs {
                    if c.starts_with("mp4a") {
                        //
                    } else if c.starts_with("opus") {
                        codec = Some(Codec::Opus);
                    } else if c.starts_with("avc1") {
                        codec = Some(Codec::H264);
                    } else if c.starts_with("vp09") {
                        codec = Some(Codec::VP9);
                    } else if c.starts_with("wvtt") {
                        codec = Some(Codec::WVTT);
                    }
                }

                // TODO: Also check role annotations like:
                // <Role schemeIdUri="urn:mpeg:dash:role:2011" value="subtitle"/>

                let mut num_variants = 0;
                if context.segment_base.is_some() {
                    num_variants += 1;
                }
                if context.segment_list.is_some() {
                    num_variants += 1;
                }
                if context.segment_template.is_some() {
                    num_variants += 1;
                }

                if num_variants > 1 {
                    return Err(err_msg(
                        "Should provide up to one of SegmentBase|SegmentList|SegmentTemplate",
                    ));
                }

                if num_variants == 0 {
                    context.segment_base = Some(SegmentBase::default());
                }

                let mut init_segment = None;
                let mut segments = vec![];

                if let Some(segment_base) = &context.segment_base {
                    if let Some(init) = &segment_base.children.initialization {
                        if init.sourceURL.is_some() {
                            return Err(err_msg(
                                "Separate init segments with SegmentBase not supported",
                            ));
                        }
                    }

                    let base_url = context
                        .base_urls
                        .get(0)
                        .ok_or_else(|| err_msg("No BaseURLs"))?;

                    segments.push(MediaTrackSegment {
                        url: base_url.value.clone(),
                        byte_range: base_url.byteRange.clone(),
                        // TODO: Add the index range.
                    });
                } else if let Some(segment_list) = &context.segment_list {
                    //

                    return Err(err_msg("SegmentList not supported"));
                } else if let Some(segment_template) = &context.segment_template {
                    // TODO: Check for byte range attributes?

                    let start_number = segment_template.base.startNumber.unwrap_or(1);

                    let timescale = segment_template
                        .base
                        .base
                        .timescale
                        .ok_or_else(|| err_msg("No timescale on SegmentTemplate"))?;

                    let mut num_variants = 0;
                    if segment_template.base.duration.is_some() {
                        num_variants += 1;
                    }
                    if segment_template.children.base.segment_timeline.is_some() {
                        num_variants += 1;
                    }

                    if num_variants != 1 {
                        return Err(err_msg(
                            "Expected exactly one of SegmentTimeline or duration",
                        ));
                    }

                    // Total period duration in the timescale units.
                    let mut total_duration = (period_duration.as_secs() * (timescale as u64))
                        + (((period_duration.subsec_micros() as u64) * (timescale as u64))
                            / 1000000);

                    let mut tmpl_inputs = SegmentTemplateInputs::default();
                    tmpl_inputs.bandwidth = Some(repr.bandwidth);
                    tmpl_inputs.representation_id = Some(repr.id.as_str());

                    if let Some(init) = &segment_template.initialization {
                        let tmpl = SegmentTemplateStr::parse_from(init.as_str())?;

                        let url = tmpl.format(&tmpl_inputs)?;

                        init_segment = Some(MediaTrackSegment {
                            url: context.resolve_uri(&url)?,
                            byte_range: None,
                        });
                    }

                    let media_tmpl = SegmentTemplateStr::parse_from(
                        segment_template
                            .media
                            .as_ref()
                            .ok_or_else(|| err_msg("SegmentTemplate missing media"))?
                            .as_str(),
                    )?;

                    if let Some(duration) = segment_template.base.duration {
                        // TODO: Check not using both time and number in
                        // templates?

                        let mut current_time = 0;
                        while current_time < total_duration {
                            tmpl_inputs.number = Some(segments.len() + start_number);
                            tmpl_inputs.time = Some(current_time as usize);

                            let url = media_tmpl.format(&tmpl_inputs)?;

                            segments.push(MediaTrackSegment {
                                url: context.resolve_uri(&url)?,
                                byte_range: None,
                            });

                            current_time += duration as u64;
                        }
                    } else if let Some(timeline) = &segment_template.children.base.segment_timeline
                    {
                        // TODO: Support specifying discontinuities in the templates.

                        let mut current_time = 0;

                        for seg in &timeline.children.s[..] {
                            if let Some(t) = seg.t {
                                if t != current_time {
                                    return Err(err_msg("Segment discontinuity"));
                                }
                            }

                            let duration = seg.d;

                            for i in 0..(1 + seg.r.unwrap_or(0)) {
                                tmpl_inputs.number = Some(segments.len() + start_number);
                                tmpl_inputs.time = Some(current_time);

                                let url = media_tmpl.format(&tmpl_inputs)?;
                                segments.push(MediaTrackSegment {
                                    url: context.resolve_uri(&url)?,
                                    byte_range: None,
                                });

                                current_time += duration;
                            }
                        }

                        if current_time as u64 != total_duration {
                            return Err(err_msg(
                                "SegmentTemplate doesn't cover the entire period duration",
                            ));
                        }
                    }
                }

                let mut content_protection_els = vec![];
                content_protection_els.extend(
                    adaptation_set
                        .children
                        .base
                        .content_protection
                        .deref()
                        .clone()
                        .into_iter(),
                );
                content_protection_els
                    .extend(repr.children.base.content_protection.deref().clone());

                let mut content_protection = vec![];

                // TODO: Error out if we get multiple <ContentProtection> boxes with the same
                // schemeIdUri.

                for el in &content_protection_els[..] {
                    if el.schemeIdUri == WIDEVINE_ID_URI {
                        let mut pssh = vec![];

                        for b in &el.children.cenc_pssh {
                            let pssh_box = base_radix::base64_decode(&b.value)?;
                            pssh.push(pssh_box);
                        }

                        content_protection.push(ContentProtection::Widevine(
                            WidevineContentProtection { pssh },
                        ));
                    }

                    if el.schemeIdUri == "urn:mpeg:dash:mp4protection:2011"
                        && el.value == Some("cenc".to_string())
                    {
                        let mut default_key_id = None;
                        if let Some(kid) = &el.cenc_default_kid {
                            default_key_id =
                                Some(uuid::UUID::parse(kid.as_str())?.as_ref().to_vec());
                        }

                        content_protection.push(ContentProtection::CENC(CENCContentProtection {
                            default_key_id,
                        }));
                    }
                }

                if content_protection.is_empty() && !content_protection_els.is_empty() {
                    return Err(err_msg(
                        "All content protection options have unknown format",
                    ));
                }

                tracks.push(MediaTrack {
                    start: period_start_time,
                    duration: period_duration,
                    kind,
                    metadata: MediaTrackMetadata {
                        language: adaptation_set.lang.clone(),
                        bandwidth: repr.bandwidth,
                        frame_size: {
                            if repr.base.width.is_some() && repr.base.height.is_some() {
                                Some(crate::FrameSize {
                                    width: repr.base.width.unwrap(),
                                    height: repr.base.height.unwrap(),
                                })
                            } else {
                                None
                            }
                        },
                        audio_bitrate: repr.base.audioSamplingRate,
                        codec,
                        container,
                    },
                    init_segment,
                    segments,
                    content_protection,
                });
            }
        }
    }

    Ok(MediaSource { tracks })
}

#[derive(Clone)]
struct MPDContext {
    base_urls: Vec<BaseURL>,
    segment_base: Option<SegmentBase>,
    segment_list: Option<SegmentList>,
    segment_template: Option<SegmentTemplate>,
    repr_base: RepresentationBaseAttributes,
}

impl MPDContext {
    fn merge_base_urls(&mut self, base_urls: &[BaseURL]) -> Result<()> {
        if base_urls.is_empty() {
            return Ok(());
        }

        let mut new_urls: Vec<BaseURL> = vec![];

        for base_url in base_urls {
            let uri = base_url.value.parse::<Uri>()?;

            if uri.scheme.is_some() {
                // Got an absolute uri
                new_urls.push(base_url.clone());
                continue;
            }

            // Otherwise merge with parent uris.

            for parent_uri in &self.base_urls {
                let value = parent_uri.value.parse::<Uri>()?.join(&uri)?;
                let byte_range = base_url.byteRange.clone().or(parent_uri.byteRange.clone());

                new_urls.push(BaseURL {
                    byteRange: byte_range,
                    value: value.to_string()?,
                });
            }
        }

        self.base_urls = new_urls;

        Ok(())
    }

    fn resolve_uri(&self, uri: &str) -> Result<String> {
        let mut uri = uri.parse::<Uri>()?;
        if !uri.scheme.is_some() {
            let base_url = self
                .base_urls
                .get(0)
                .ok_or_else(|| err_msg("No BaseURLs"))?;

            uri = base_url.value.parse::<Uri>()?.join(&uri)?;
        }

        Ok(uri.to_string()?)
    }

    fn merge_segment_base(&mut self, segment_base: &Option<SegmentBase>) -> Result<()> {
        if segment_base.is_none() {
            return Ok(());
        }

        if self.segment_base.is_some() {
            return Err(err_msg("Combining SegmentBase values not support"));
        }

        self.segment_base = segment_base.clone();
        Ok(())
    }

    fn merge_segment_list(&mut self, segment_list: &Option<SegmentList>) -> Result<()> {
        if segment_list.is_none() {
            return Ok(());
        }

        if self.segment_list.is_some() {
            return Err(err_msg("Combining SegmentList values not support"));
        }

        self.segment_list = segment_list.clone();
        Ok(())
    }

    fn merge_segment_template(&mut self, segment_template: &Option<SegmentTemplate>) -> Result<()> {
        if segment_template.is_none() {
            return Ok(());
        }

        if self.segment_template.is_some() {
            return Err(err_msg("Combining SegmentTemplate values not support"));
        }

        self.segment_template = segment_template.clone();
        Ok(())
    }

    fn merge_repr_base(&mut self, repr_base: &RepresentationBaseAttributes) {
        self.repr_base.merge(repr_base);
    }
}
