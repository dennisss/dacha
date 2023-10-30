// Format of MPD files.
// Note that we don't support some features so we convert unknown attributes
// into parsing errors to avoid incorrectly missing attributes that are critical
// to playing the content.

use common::errors::*;

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct MPD {
    pub id: Option<String>,
    pub profiles: String,
    pub mediaPresentationDuration: Option<xml::Duration>,
    pub minBufferTime: xml::Duration,

    /// Default value is "static". Can be changed to "dynamic"
    #[parse(name = "type")]
    pub typ: Option<String>,

    #[parse(name = "$content")]
    pub children: MDPChildren,
}

#[derive(Parseable, Debug)]
pub struct MDPChildren {
    #[parse(name = "Period")]
    pub periods: xml::List<Period>,

    #[parse(name = "BaseURL")]
    pub base_url: xml::List<BaseURL>,
}

#[derive(Parseable, Debug)]
pub struct Period {
    pub id: Option<String>,
    pub start: Option<xml::Duration>,
    pub duration: Option<xml::Duration>,

    #[parse(name = "$content")]
    pub children: PeriodChildren,
}

#[derive(Parseable, Debug)]
pub struct PeriodChildren {
    #[parse(name = "AdaptationSet")]
    pub adaptation_sets: xml::List<AdaptationSet>,

    #[parse(name = "BaseURL")]
    pub base_url: xml::List<BaseURL>,

    #[parse(name = "SegmentBase")]
    pub segment_base: Option<SegmentBase>,

    #[parse(name = "SegmentList")]
    pub segment_list: Option<SegmentList>,

    #[parse(name = "SegmentTemplate")]
    pub segment_template: Option<SegmentTemplate>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct AdaptationSet {
    #[parse(flatten = true)]
    pub base: RepresentationBaseAttributes,

    pub id: Option<String>,
    pub group: Option<usize>,
    pub lang: Option<String>,
    pub contentType: Option<String>,
    pub par: Option<String>,
    pub minBandwidth: Option<usize>,
    pub maxBandwidth: Option<usize>,
    pub minWidth: Option<usize>,
    pub maxWidth: Option<usize>,
    pub minHeight: Option<usize>,
    pub maxHeight: Option<usize>,
    pub subsegmentAlignment: Option<String>, // bool

    #[parse(name = "$content")]
    pub children: AdaptationSetChildren,
}

#[derive(Parseable, Debug)]

pub struct AdaptationSetChildren {
    #[parse(flatten = true)]
    pub base: RepresentationBaseChildren,

    #[parse(name = "Representation")]
    pub representations: xml::List<Representation>,

    #[parse(name = "Role")]
    pub roles: xml::List<Descriptor>,

    #[parse(name = "BaseURL")]
    pub base_url: xml::List<BaseURL>,

    #[parse(name = "SegmentBase")]
    pub segment_base: Option<SegmentBase>,

    #[parse(name = "SegmentList")]
    pub segment_list: Option<SegmentList>,

    #[parse(name = "SegmentTemplate")]
    pub segment_template: Option<SegmentTemplate>,
}

#[derive(Parseable, Debug)]
pub struct Representation {
    #[parse(flatten = true)]
    pub base: RepresentationBaseAttributes,
    pub id: String,
    pub bandwidth: usize,
    // <xs:attribute name="qualityRanking" type="xs:unsignedInt"/>
    // <xs:attribute name="dependencyId" type="StringVectorType"/>
    // <xs:attribute name="mediaStreamStructureId" type="StringVectorType"/>
    #[parse(name = "$content")]
    pub children: RepresentationChildren,
}

#[derive(Parseable, Debug)]
pub struct RepresentationChildren {
    #[parse(flatten = true)]
    pub base: RepresentationBaseChildren,

    #[parse(name = "BaseURL")]
    pub base_url: xml::List<BaseURL>,

    #[parse(name = "SubRepresentation")]
    pub sub_representations: xml::List<SubRepresentation>,

    #[parse(name = "SegmentBase")]
    pub segment_base: Option<SegmentBase>,

    #[parse(name = "SegmentList")]
    pub segment_list: Option<SegmentList>,

    #[parse(name = "SegmentTemplate")]
    pub segment_template: Option<SegmentTemplate>,
}

#[derive(Parseable, Debug)]
pub struct SubRepresentation {
    // TODO
}

#[derive(Parseable, Debug, Clone, Default)]
pub struct RepresentationBaseAttributes {
    pub profiles: Option<String>,
    pub width: Option<usize>,
    pub height: Option<usize>,
    pub sar: Option<String>,
    pub frameRate: Option<String>,
    pub audioSamplingRate: Option<usize>,
    pub mimeType: Option<String>,
    pub segmentProfiles: Option<String>,
    pub codecs: Option<String>,
    // <xs:attribute name="maximumSAPPeriod" type="xs:double"/>
    // <xs:attribute name="startWithSAP" type="SAPType"/>
    // <xs:attribute name="maxPlayoutRate" type="xs:double"/>
    // <xs:attribute name="codingDependency" type="xs:boolean"/>
    // <xs:attribute name="scanType" type="VideoScanType"/>
}

impl RepresentationBaseAttributes {
    pub fn merge(&mut self, other: &RepresentationBaseAttributes) {
        self.profiles = other.profiles.clone().or(self.profiles.take());
        self.width = other.width.clone().or(self.width.take());
        self.sar = other.sar.clone().or(self.sar.take());
        self.frameRate = other.frameRate.clone().or(self.frameRate.take());
        self.audioSamplingRate = other
            .audioSamplingRate
            .clone()
            .or(self.audioSamplingRate.take());
        self.mimeType = other.mimeType.clone().or(self.mimeType.take());
        self.segmentProfiles = other
            .segmentProfiles
            .clone()
            .or(self.segmentProfiles.take());
        self.codecs = other.codecs.clone().or(self.codecs.take());
    }
}

#[derive(Parseable, Debug)]
pub struct RepresentationBaseChildren {
    // xs:element name="FramePacking" type="DescriptorType" minOccurs="0" maxOccurs="unbounded"/>
    #[parse(name = "AudioChannelConfiguration")]
    pub audio_channel_configuration: xml::List<Descriptor>,

    #[parse(name = "ContentProtection")]
    pub content_protection: xml::List<Descriptor>,
}

#[derive(Parseable, Debug, Clone, Default)]
pub struct SegmentBase {
    #[parse(flatten = true)]
    pub attrs: SegmentBaseAttributes,

    // <xs:attribute name="availabilityTimeOffset" type="xs:double"/>
    // <xs:attribute name="availabilityTimeComplete" type="xs:boolean"/>
    #[parse(name = "$content")]
    pub children: SegmentBaseChildren,
}

#[derive(Parseable, Debug, Clone, Default)]
pub struct SegmentBaseAttributes {
    pub timescale: Option<usize>,

    // TODO: Implement support for this.
    pub presentationTimeOffset: Option<u64>,

    pub indexRange: Option<String>,
    pub indexRangeExact: Option<String>, // bool (sparse)
}

#[derive(Parseable, Debug, Clone, Default)]
pub struct SegmentBaseChildren {
    #[parse(name = "Initialization")]
    pub initialization: Option<URL>,

    #[parse(name = "RepresentationIndex")]
    pub representation_index: Option<URL>,
}

// TODO: Inherits from SegmentBase
#[derive(Parseable, Debug, Clone)]
pub struct MultipleSegmentBaseAttributes {
    pub duration: Option<usize>,
    pub startNumber: Option<usize>,
    #[parse(flatten = true)]
    pub base: SegmentBaseAttributes,
}

// TODO: Inherits from SegmentBase
#[derive(Parseable, Debug, Clone, Default)]
pub struct MultipleSegmentBaseChildren {
    #[parse(name = "SegmentTimeline")]
    pub segment_timeline: Option<SegmentTimeline>,
}

#[derive(Parseable, Debug, Clone)]
pub struct URL {
    pub sourceURL: Option<String>,
    pub range: Option<String>,
}

#[derive(Parseable, Debug, Clone)]
pub struct SegmentTimeline {
    /// NOTE: There will always be at least one of these.
    #[parse(name = "$content")]
    pub children: SegmentTimelineChildren,
}

#[derive(Parseable, Debug, Clone)]
pub struct SegmentTimelineChildren {
    pub s: xml::List<SegmentTimelineElement>,
}

#[derive(Parseable, Debug, Clone)]
pub struct SegmentTimelineElement {
    pub t: Option<usize>,
    pub n: Option<usize>,
    pub d: usize,
    pub r: Option<usize>,
}

#[derive(Parseable, Debug, Clone)]

pub struct SegmentList {
    // TODO
}

// #[derive(Parseable, Debug)]
// pub struct SegmentListChildren {
//     #[parse(name = "SegmentURL")]
//     pub urls: xml::List<SegmentURL>
// }

// TODO: Inherit from MultipleSegmentBaseType
#[derive(Parseable, Debug, Clone)]

pub struct SegmentTemplate {
    pub media: Option<String>,
    pub index: Option<String>,
    pub initialization: Option<String>,
    pub bitstreamSwitching: Option<String>,
    #[parse(flatten = true)]
    pub base: MultipleSegmentBaseAttributes,
    #[parse(name = "$content", sparse = true)]
    pub children: SegmentTemplateChildren,
}

#[derive(Parseable, Debug, Clone, Default)]
pub struct SegmentTemplateChildren {
    #[parse(flatten = true)]
    pub base: MultipleSegmentBaseChildren,
}

#[derive(Parseable, Debug, Clone)]
pub struct BaseURL {
    #[parse(name = "$content")]
    pub value: String,

    pub byteRange: Option<String>,
}

#[derive(Parseable, Debug, Clone)]
pub struct Descriptor {
    pub schemeIdUri: String,
    pub id: Option<String>,
    pub value: Option<String>,

    #[parse(name = "cenc:default_KID")]
    pub cenc_default_kid: Option<String>,

    #[parse(name = "xmlns:cenc")]
    pub xmlns_cenc: Option<String>,

    #[parse(name = "$content", sparse = true)]
    pub children: DescriptorChildren,
}

#[derive(Parseable, Debug, Default, Clone)]
pub struct DescriptorChildren {
    #[parse(name = "cenc:pssh")]
    pub cenc_pssh: Option<StringElement>,
}

#[derive(Parseable, Debug, Clone)]
pub struct StringElement {
    #[parse(name = "$content")]
    pub value: String,

    #[parse(name = "xmlns:cenc")]
    pub xmlns_cenc: Option<String>,
}

/*
    <AdaptationSet id="0" contentType="text" lang="en" subsegmentAlignment="true">
      <Role schemeIdUri="urn:mpeg:dash:role:2011" value="main"/>
      <Representation id="0" bandwidth="256" mimeType="text/vtt">
        <BaseURL>s-en.webvtt</BaseURL>
      </Representation>
    </AdaptationSet>

    <AdaptationSet id="15" contentType="audio" lang="es" subsegmentAlignment="true">
      <ContentProtection schemeIdUri="urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed" cenc:default_KID="67b30c86-756f-57c5-a0a3-8a23ac8c9178">
        <cenc:pssh>AAAAPnBzc2gAAAAA7e+LqXnWSs6jyCfc1R0h7QAAAB4iFnNoYWthX2NlYzJmNjRhYTc4OTBhMTFI49yVmwY=</cenc:pssh>
      </ContentProtection>
      <Representation id="25" bandwidth="152381" codecs="opus" mimeType="audio/webm" audioSamplingRate="48000">
        <AudioChannelConfiguration schemeIdUri="urn:mpeg:dash:23003:3:audio_channel_configuration:2011" value="2"/>
        <BaseURL>a-spa-0096k-libopus-2c.webm</BaseURL>
        <SegmentBase indexRange="377-635" timescale="1000000">
          <Initialization range="0-376"/>
        </SegmentBase>
      </Representation>
    </AdaptationSet>


    <AdaptationSet mimeType="video/mp4" codecs="avc1.42c00d">
        <SegmentTemplate media="../video/$RepresentationID$/cenc_dash/segment_$Number$.m4s" initialization="../video/$RepresentationID$/cenc_dash/init.mp4" duration="4000" startNumber="0" timescale="1000"/>
        <Representation id="180_250000" bandwidth="250000" width="320" height="180" frameRate="25">
            <ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011" value="cenc" cenc:default_KID="eb676abb-cb34-5e96-bbcf-616630f1a3da" xmlns:cenc="urn:mpeg:cenc:2013"/>
            <ContentProtection schemeIdUri="urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed">
                <cenc:pssh xmlns:cenc="urn:mpeg:cenc:2013">AAAAW3Bzc2gAAAAA7e+LqXnWSs6jyCfc1R0h7QAAADsIARIQ62dqu8s0Xpa7z2FmMPGj2hoNd2lkZXZpbmVfdGVzdCIQZmtqM2xqYVNkZmFsa3IzaioCSEQyAA==</cenc:pssh>
            </ContentProtection>
        </Representation>
    </...>
*/
