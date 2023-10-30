use std::time::Duration;

#[derive(Debug)]
pub struct MediaSource {
    pub tracks: Vec<MediaTrack>,
}

#[derive(Debug)]
pub struct MediaTrack {
    pub start: Duration,
    pub duration: Duration,
    pub kind: MediaTrackKind,
    pub metadata: MediaTrackMetadata,
    pub init_segment: Option<MediaTrackSegment>,
    pub segments: Vec<MediaTrackSegment>,
    pub content_protection: Vec<ContentProtection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaTrackKind {
    Unknown,
    Video,
    Audio,
    Subtitle,
}

#[derive(Debug)]
pub struct MediaTrackMetadata {
    pub language: Option<String>,
    pub bandwidth: usize,
    pub frame_size: Option<FrameSize>,
    pub audio_bitrate: Option<usize>,
    pub codec: Option<Codec>,
    pub container: Option<Container>,
}

#[derive(Debug)]
pub struct FrameSize {
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum Codec {
    AAC,
    Opus,
    VP9,
    H264,
    H265,
    AV1,
    WVTT,
}

#[derive(Debug)]
pub enum Container {
    MP4,
    WebM,
}

#[derive(Debug)]
pub struct MediaTrackSegment {
    pub url: String,
    pub byte_range: Option<String>,
}

#[derive(Debug)]
pub enum ContentProtection {
    Widevine(WidevineContentProtection),
    CENC(CENCContentProtection),
}

#[derive(Debug)]
pub struct WidevineContentProtection {
    pub pssh: Vec<Vec<u8>>,
}

#[derive(Debug)]

pub struct CENCContentProtection {
    pub default_key_id: Option<Vec<u8>>,
}
