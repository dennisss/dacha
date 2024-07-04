use std::{collections::VecDeque, time::Duration};

use common::errors::*;
use compression::transform::{Transform, TransformProgress};

mod proto {
    include!(concat!(env!("OUT_DIR"), "/src/mp4.rs"));

    impl BoxClass {
        pub fn children(&self) -> &[BoxClass] {
            match &self.value {
                BoxData::FileTypeBox(_)
                | BoxData::SegmentTypeBox(_)
                | BoxData::MovieHeaderBox(_)
                | BoxData::TrackHeaderBox(_)
                | BoxData::TrackFragmentHeaderBox(_)
                | BoxData::TrackFragmentBaseMediaDecodeTimeBox(_)
                | BoxData::TrackRunBox(_)
                | BoxData::MediaHeaderBox(_)
                | BoxData::HandlerBox(_)
                | BoxData::MovieExtendsHeaderBox(_)
                | BoxData::TrackExtendsBox(_)
                | BoxData::VideoMediaHeaderBox(_)
                | BoxData::TimeToSampleBox(_)
                | BoxData::SyncSampleBox(_)
                | BoxData::EditListBox(_)
                | BoxData::DataEntryUrlBox(_)
                | BoxData::ChunkOffsetBox(_)
                | BoxData::SampleToChunkBox(_)
                | BoxData::SampleSizeBox(_)
                | BoxData::AVCDecoderConfigurationRecord(_)
                | BoxData::MovieFragmentHeaderBox(_)
                | BoxData::SampleEncryptionBox(_)
                | BoxData::Unknown(_) => &[],

                BoxData::MovieBox(v)
                | BoxData::TrackBox(v)
                | BoxData::MediaBox(v)
                | BoxData::MediaInformationBox(v)
                | BoxData::SampleTableBox(v)
                | BoxData::EditBox(v)
                | BoxData::DataInformationBox(v)
                | BoxData::UserDataBox(v)
                | BoxData::MovieExtendsBox(v)
                | BoxData::MovieFragmentBox(v)
                | BoxData::TrackFragmentBox(v) => &v.children,

                BoxData::AVC1(v) => &v.children,
                BoxData::EncV(v) => &v.children,
                BoxData::SampleDescriptionBox(v) => &v.children,
                BoxData::DataReferenceBox(v) => &v.children,
            }
        }

        pub fn children_mut(&mut self) -> Option<&mut Vec<BoxClass>> {
            match &mut self.value {
                BoxData::FileTypeBox(_)
                | BoxData::SegmentTypeBox(_)
                | BoxData::MovieHeaderBox(_)
                | BoxData::TrackHeaderBox(_)
                | BoxData::TrackFragmentHeaderBox(_)
                | BoxData::TrackFragmentBaseMediaDecodeTimeBox(_)
                | BoxData::TrackRunBox(_)
                | BoxData::MediaHeaderBox(_)
                | BoxData::HandlerBox(_)
                | BoxData::MovieExtendsHeaderBox(_)
                | BoxData::TrackExtendsBox(_)
                | BoxData::VideoMediaHeaderBox(_)
                | BoxData::TimeToSampleBox(_)
                | BoxData::SyncSampleBox(_)
                | BoxData::EditListBox(_)
                | BoxData::DataEntryUrlBox(_)
                | BoxData::ChunkOffsetBox(_)
                | BoxData::SampleToChunkBox(_)
                | BoxData::SampleSizeBox(_)
                | BoxData::AVCDecoderConfigurationRecord(_)
                | BoxData::MovieFragmentHeaderBox(_)
                | BoxData::SampleEncryptionBox(_)
                | BoxData::Unknown(_) => None,

                BoxData::MovieBox(v)
                | BoxData::TrackBox(v)
                | BoxData::MediaBox(v)
                | BoxData::MediaInformationBox(v)
                | BoxData::SampleTableBox(v)
                | BoxData::EditBox(v)
                | BoxData::DataInformationBox(v)
                | BoxData::UserDataBox(v)
                | BoxData::MovieExtendsBox(v)
                | BoxData::MovieFragmentBox(v)
                | BoxData::TrackFragmentBox(v) => Some(&mut v.children),

                BoxData::AVC1(v) => Some(&mut v.children),
                BoxData::EncV(v) => Some(&mut v.children),
                BoxData::SampleDescriptionBox(v) => Some(&mut v.children),
                BoxData::DataReferenceBox(v) => Some(&mut v.children),
            }
        }

        pub fn visit_children<F: FnMut(&BoxClass) -> Result<()>>(&self, mut f: F) -> Result<()> {
            fn inner<F: FnMut(&BoxClass) -> Result<()>>(b: &BoxClass, f: &mut F) -> Result<()> {
                f(b)?;

                for c in b.children() {
                    inner(c, f)?;
                }

                Ok(())
            }

            inner(self, &mut f)
        }

        pub fn visit_children_mut<F: FnMut(&mut BoxClass) -> Result<()>>(
            &mut self,
            mut f: F,
        ) -> Result<()> {
            fn inner<F: FnMut(&mut BoxClass) -> Result<()>>(
                b: &mut BoxClass,
                f: &mut F,
            ) -> Result<()> {
                f(b)?;

                if let Some(children) = b.children_mut() {
                    for c in children {
                        inner(c, f)?;
                    }
                }

                Ok(())
            }

            inner(self, &mut f)
        }
    }
}

pub use proto::*;

use crate::h264::*;

const MEDIA_TIMESCALE: u64 = 1_200_000;
const MOVIE_TIMESCALE: u64 = 1_000;

/// Options for configuring how MP4s are built.
///
/// The default settings will construct a single MP4 that contains all the media
/// data in one 'mdat' box with the 'moov' box (containing the seeking
/// information) at the very end of the file.
///
/// Recommended settings for different usecases:
/// - VOD: Default settings
///   - Note that the 'moov' will be at the end of the file, so it helps to use
///     HLS/DASH for hinting the location of the index.
/// - Streaming: Set 'fragment' to ~10s and use 'buffer_data'
///    - The first generated file will always be the 'init' segment.
///    - Use with HLS/DASH to coordinate many fragments.
#[derive(Defaultable)]
pub struct MP4BuilderOptions {
    /// If not none, then fragment the MP4 at intervals of roughly this many
    /// samples/frames. The actual fragment size may:
    /// - Increase to align with the next key frame boundary.
    /// - Decrease to fit within the max_chunk_size
    ///
    /// Only individual fragments are seekable while seeking across fragments
    /// requires scanning all the fragments.
    pub fragment: Option<usize>,

    /// Maximum size of one 'mdat' box in bytes. Because the builder operates on
    /// append-only files, this will be the max size of memory allocated by the
    /// MP4 builder.
    #[default(16 * 1024 * 1024)]
    pub max_chunk_size: usize,

    /// If true, if the first frame is not an key frame, then skip ahead (ignore
    /// frames) until we see the first key frame. Else, we will return an error.
    pub skip_to_key_frame: bool,

    /// Target segment size in number of bytes.
    pub max_segment_size: Option<u64>,

    /// If true, then every segment will be independently playable:
    /// - Each will start with an 'init' chunk.
    /// - Fragment sequence numbers and base times are reset to zero at the
    ///   beginning of each segment.
    ///
    /// If not using fragmentation, then this is implicitly true.
    pub independent_segments: bool,
}

/// When fragmentation is enabled, each of these corresponds to one fragment.
pub struct MP4BuilderChunk {
    /// Starts at zero. All chunks with the same segment_index should be written
    /// to the same file.
    pub segment_index: usize,

    pub is_init: bool,

    pub data: Vec<u8>,

    /// Time range that data in this chunk represents.
    ///
    /// - This will always start at time 0 for the first fragment and will
    ///   sometimes reset back to 0 if independent_segments are built.
    /// - Note that this chunk will only be individually seekable if
    ///   fragmentation is enabled.
    /// - This may accumulate up to 1 microsecond of error compared to the user
    ///   provided frame timed per fragment / independent segment.
    pub time_range: Option<TimeRange>,

    /// User provided timestamp for the first frame in this chunk.
    /// This will match the time passed into the append() function
    pub user_time_range: Option<TimeRange>,
}

pub struct TimeRange {
    pub start: Duration,
    pub end: Duration,
}

/// Builds MP4s. Note that this is a streaming builder and will never edit old
/// data.
///
/// The output is generated as a sequence of chunks (/ fragments). Groups of
/// these chunks/fragments are optionally grouped into segments (files).
/// They will be structured in one of two forms:
///
/// 1. Non-Fragmented (Single-File) (when options.fragment.is_none()):
///   - First Chunk:
///     - 'ftyp'
///     - 'mdat' (0 or 1)
///   - Middle chunks:
///     - 'mdat'
///   - Last Chunk
///     - 'mdat' (0 or 1)
///     - 'moov'
/// 2. Fragmented (when options.fragment.is_some())
///   - First Chunk (init segment):
///     - 'ftyp'
///     - 'moov' (with 'mvex' box)
///   - Remaining Chunks:
///     - 'styp'
///     - 'moof'
///     - 'mdat'
///
/// In form #1, separate segments will each have their own First/Middle/Last
/// chunks.
///
/// In form #2, only the first segment will have the init chunk (unless
/// independent_segments is true).
pub struct MP4Builder {
    options: MP4BuilderOptions,

    // TODO: Do more inference of these based on the SPS/PPS
    frame_width: u32,
    frame_height: u32,
    frame_rate: u32,

    params: Parameters,

    /// Fully built chunks waiting to be consumed by the user via calls to
    /// consume().
    output: VecDeque<MP4BuilderChunk>,

    /// Index of the current segment/file being written.
    segment_index: usize,

    /// Buffer used for building complete MP4 boxes once sufficient data has
    /// been accumulated in the chunk_buffer. This buffer will be cleared and
    /// appended to 'output' once ready for consumption.
    ///
    /// TODO: It would be better to make this a rope so that we don't need to
    /// re-allocate the buffer continously.
    segment_buffer: Vec<u8>,

    /// Offset at which the first byte of the output_buffer will be written to
    /// in the current file being written.
    ///
    /// (this is only used in non-fragmented single file mode)
    ///
    /// May be negative if previously written files weren't consumed by the
    /// caller yet.
    segment_file_offset: u64,

    /// State associated with one contiguous track (starting at timecode
    /// 0:00 in the video).
    track: TrackState,
}

#[derive(Default)]
struct TrackState {
    /// Timestamp (using the media timescale) at which the current
    /// fragment/chunk (that is still being built) starts at.
    media_base_time: u64,

    /// Number of samples in sample_sizes/sample_times which are from previous
    /// chunks.
    media_base_num_samples: usize,

    /// Sequence number of the current fragment being written.
    /// 0 is used as a special value to signify the 'init' fragment.
    ///
    /// (only used if fragmented mode is enabled).
    fragment_number: usize,

    /// Each chunk is a (file_offset, num_samples)
    chunks: Vec<(usize, usize)>,

    /// Buffer for data being added to the current chunk/fragment's mdat box.
    chunk_buffer: Vec<u8>,

    /// Byte size of each sample in the current fragment.
    ///
    /// Each sample is 1 H264 NALUs (IDR/non-IDR picture) with a 4-byte length
    /// prefixes.
    sample_sizes: Vec<u32>,

    /// User provided timestamps for each sample/frame.
    sample_times: Vec<Duration>,

    /// Sorted list of sample indices in the current fragment which contain
    /// key/I-frames.
    ///
    /// NOTE: Sample indexes start at 1 (not 0).
    sample_key_indices: Vec<u32>,
}

/// Runtime resolved media properties.
#[derive(Default, Clone)]
struct Parameters {
    sequence_parameter_set: Option<Vec<u8>>,
    picture_parameter_set: Option<Vec<u8>>,
}

impl MP4Builder {
    /// Creates a new empty instance for building an MP4.
    ///
    /// - frame_rate: frames per second in the video track.
    pub fn new(
        frame_width: u32,
        frame_height: u32,
        frame_rate: u32,
        options: MP4BuilderOptions,
    ) -> Result<Self> {
        Ok(Self {
            options,
            frame_height,
            frame_width,
            frame_rate,
            params: Parameters::default(),
            output: VecDeque::new(),
            segment_index: 0,
            segment_buffer: vec![],
            segment_file_offset: 0,
            track: TrackState::default(),
        })
    }

    /// Gets the complete mime type of the file including codec information.
    ///
    /// See https://developer.mozilla.org/en-US/docs/Web/Media/Formats/codecs_parameter
    /// - For H264, the codec specifier is of the form "avc1[.PPCCLL]"
    ///     - PP : Profile number
    ///     - CC : constraint set flags
    ///     - LL : level
    ///     where the level is a fixed point number
    ///     (0x14 is 20 in decimal which means 2.0)
    ///
    /// So a typical return value would look like `video/mp4;
    /// codecs="avc1.4D401F"`
    ///
    /// See also the list of Chrome supported IDCs
    /// https://source.chromium.org/chromium/chromium/src/+/main:media/video/h264_parser.cc
    ///
    /// This will return fail if codec information hasn't been appended yet or
    /// it is invalid.
    pub fn mime_type(&self) -> Result<String> {
        let sps = self
            .params
            .sequence_parameter_set
            .clone()
            .ok_or_else(|| err_msg("Empty video: Missing SPS"))?;

        // Parsing critical bytes in the SPS
        if sps.len() < 4 {
            return Err(err_msg("SPS is too small"));
        }

        // TODO: Deduplicate this parsing code.
        // NOTE: sps[0] is the NALUnitHeader
        let profile_idc = sps[1];
        let profile_compatibility = sps[2];
        let level_idc = sps[3];

        Ok(format!(
            "video/mp4; codecs=\"avc1.{:02X}{:02X}{:02X}\"",
            profile_idc, profile_compatibility, level_idc
        ))
    }

    /// Appends bytes of a raw 'H264 with start codes' stream to the MP4. The
    /// data is buffered in memory until the MP4 is fully built.
    ///
    /// - h264_stream should be raw H264 bit stream data for exactly one
    ///   complete frame.
    /// - frame_time should be the time at which the frame was captured. The
    ///   origin doesn't matter as only the relative time between frames is
    ///   used.
    ///   - The previous frame will be displayed for current.frame_time -
    ///     last.frame_time.
    ///   - The final frame in the MP4 will be displayed for `1 second /
    ///     frame_rate``
    pub fn append(
        &mut self,
        h264_stream: &[u8],
        frame_time: Option<Duration>,
        end_of_input: bool,
    ) -> Result<()> {
        // TODO: This can't tell if the frame has complete packets or not.
        let mut iter = H264BitStreamIterator::new(h264_stream);

        let default_interval = Duration::from_secs_f32(1.0 / (self.frame_rate as f32));

        let mut sample_time = match frame_time {
            Some(v) => v,
            None => {
                self.track
                    .sample_times
                    .last()
                    .cloned()
                    .unwrap_or(Duration::ZERO)
                    + default_interval
            }
        };

        let mut got_frame_data = false;

        while let Some(nalu) = iter.peek() {
            let (header, rest) = NALUnitHeader::parse(nalu.data())?;
            match header.nal_unit_type {
                NALUnitType::PPS => {
                    let params = nalu.data();

                    if let Some(old_params) = &self.params.picture_parameter_set {
                        if old_params != params {
                            return Err(err_msg("PPS changed part way through video"));
                        }
                    } else {
                        self.params.picture_parameter_set = Some(params.to_vec());
                    }
                }
                NALUnitType::SPS => {
                    let params = nalu.data();

                    if let Some(old_params) = &self.params.sequence_parameter_set {
                        if old_params != params {
                            return Err(err_msg("SPS changed part way through video"));
                        }
                    } else {
                        self.params.sequence_parameter_set = Some(params.to_vec());
                    }
                }
                NALUnitType::IDRPicture => {
                    if got_frame_data && frame_time.is_some() {
                        return Err(err_msg("More than one frame in append() call."));
                    }

                    got_frame_data = true;

                    // Maybe terminate the current fragment.
                    self.maybe_finish_current_chunk(
                        Some(4 + nalu.data().len()),
                        sample_time,
                        false,
                    )?;

                    let start_size = self.track.chunk_buffer.len();
                    Self::append_nalu(nalu.data(), &mut self.track.chunk_buffer);
                    let end_size = self.track.chunk_buffer.len();

                    self.track.sample_sizes.push((end_size - start_size) as u32);
                    self.track
                        .sample_key_indices
                        .push(self.track.sample_sizes.len() as u32);
                    self.track.sample_times.push(sample_time);

                    sample_time += default_interval;
                }
                NALUnitType::NonIDRPicture => {
                    if got_frame_data && frame_time.is_some() {
                        return Err(err_msg("More than one frame in append() call."));
                    }

                    got_frame_data = true;

                    if self.track.sample_sizes.is_empty() {
                        if self.options.skip_to_key_frame {
                            nalu.advance();
                            continue;
                        }

                        return Err(err_msg(
                            "Expected the first sample to contain a IDR picture",
                        ));
                    }

                    let start_size = self.track.chunk_buffer.len();

                    Self::append_nalu(nalu.data(), &mut self.track.chunk_buffer);

                    let end_size = self.track.chunk_buffer.len();
                    self.track.sample_sizes.push((end_size - start_size) as u32);
                    self.track.sample_times.push(sample_time);

                    sample_time += default_interval;
                }
                v @ _ => {
                    return Err(format_err!("Unsupported NALU type: {:?}", v));
                }
            }

            nalu.advance();
        }

        if !iter.remaining().is_empty() {
            return Err(err_msg("Not all inputs were consumed"));
        }

        if end_of_input {
            self.maybe_finish_current_chunk(None, sample_time, true)?;
        }

        Ok(())
    }

    fn take_output_buffer(&mut self) -> Vec<u8> {
        self.segment_file_offset += self.segment_buffer.len() as u64;

        let mut buf = vec![];
        core::mem::swap(&mut buf, &mut self.segment_buffer);

        buf
    }

    /// Call when the initial PPS/SPS fragments have been received to see if we
    /// need to write the 'init' segment for fragmented MP4s.
    fn finish_init_fragment(&mut self) -> Result<()> {
        if self.options.fragment.is_none() || self.track.fragment_number != 0 {
            return Ok(());
        }

        assert!(self.track.sample_sizes.is_empty());
        assert!(self.track.media_base_time == 0);

        self.segment_buffer
            .extend_from_slice(&self.create_moov_box(Duration::ZERO)?);
        self.track.fragment_number += 1;

        let data = self.take_output_buffer();
        self.output.push_back(MP4BuilderChunk {
            segment_index: self.segment_index,
            is_init: true,
            data,
            time_range: None,
            user_time_range: None,
        });

        // TODO: In non-independent segment mode, advance to the next segment?

        Ok(())
    }

    fn create_ftyp_box(&mut self) -> Result<()> {
        proto::BoxClass {
            typ: "ftyp".into(),
            value: BoxData::FileTypeBox(FileTypeBox {
                major_brand: "isom".into(),
                minor_version: 512,
                compatible_brands: vec!["isom".into(), "iso2".into(), "avc1".into(), "mp41".into()],
            }),
        }
        .serialize(&mut self.segment_buffer)
    }

    fn create_moov_box(&self, next_frame_time: Duration) -> Result<Vec<u8>> {
        let num_samples = self.track.sample_sizes.len() as u32;

        let sps = self
            .params
            .sequence_parameter_set
            .clone()
            .ok_or_else(|| err_msg("Empty video: Missing SPS"))?;
        let pps = self
            .params
            .picture_parameter_set
            .clone()
            .ok_or_else(|| err_msg("Empty video: Missing SPS"))?;

        // Parsing critical bytes in the SPS
        if sps.len() < 4 {
            return Err(err_msg("SPS is too small"));
        }
        // NOTE: sps[0] is the NALUnitHeader
        let profile_idc = sps[1];
        let profile_compatibility = sps[2];
        let level_idc = sps[3];

        // TODO: Use 64-bit precision for this calculation.
        let movie_duration =
            (self.track.media_base_time / (MEDIA_TIMESCALE / MOVIE_TIMESCALE)) as u32;

        let media_sample_delta = (MEDIA_TIMESCALE / (self.frame_rate as u64)) as u32;

        let (sample_durations, media_duration) =
            Self::get_sample_durations(&self.track.sample_times, next_frame_time);
        assert_eq!(media_duration, self.track.media_base_time);

        // TODO: Store this in the original 64-bit precision.
        let media_duration = media_duration as u32;

        // TODO: Apply some compression/smoothing to sample_durations.

        let mut stts_entries: Vec<TimeToSampleBoxEntry> = vec![];
        for dur in sample_durations {
            if let Some(last_entry) = stts_entries.last_mut() {
                if last_entry.sample_delta == dur {
                    last_entry.sample_count += 1;
                    continue;
                }
            }

            stts_entries.push(TimeToSampleBoxEntry {
                sample_count: 1,
                sample_delta: dur,
            });
        }

        let dinf_box = proto::BoxClass {
            typ: "dinf".into(),
            value: BoxData::DataInformationBox(ContainerBox {
                children: vec![
                    proto::BoxClass {
                        typ: "dref".into(),
                        value: BoxData::DataReferenceBox(DataReferenceBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            children: vec![proto::BoxClass {
                                typ: "url ".into(),
                                value: BoxData::DataEntryUrlBox(DataEntryUrlBox {
                                    full_box_header: FullBoxHeader {
                                        version: 0,
                                        flags: 1, // Data is in the same file.
                                    },
                                    location: None,
                                }),
                            }],
                        }),
                    },
                    //
                ],
            }),
        };

        let stbl_box = proto::BoxClass {
            typ: "stbl".into(),
            value: BoxData::SampleTableBox(ContainerBox {
                children: vec![
                    proto::BoxClass {
                        typ: "stsd".into(),
                        value: BoxData::SampleDescriptionBox(SampleDescriptionBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            children: vec![proto::BoxClass {
                                typ: "avc1".into(),
                                value: BoxData::AVC1(VisualSampleEntry {
                                    sample_entry: SampleEntry {
                                        data_reference_index: 1, // TODO
                                    },
                                    width: self.frame_width as u16,
                                    height: self.frame_height as u16,
                                    horizresolution: 4718592, // 72dpi
                                    vertresolution: 4718592,  // 72dpi
                                    frame_count: 1,           // 1 frame per sample
                                    compressorname: [
                                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                    ]
                                    .into(),
                                    depth: 24,
                                    children: vec![proto::BoxClass {
                                        typ: "avcC".into(),
                                        value: BoxData::AVCDecoderConfigurationRecord(
                                            // TODO: Configurate all this stuff correctly.
                                            AVCDecoderConfigurationRecord {
                                                configuration_version: 1, // Constant
                                                avc_profile_indicator: profile_idc,
                                                profile_compatibility: profile_compatibility,
                                                avc_level_indication: level_idc,
                                                length_size_minus_one: 3, /* u32's for length
                                                                           * prefixes */
                                                sequence_parameter_sets: vec![AVCParameterSet {
                                                    data: sps,
                                                }],
                                                picture_parameter_sets: vec![AVCParameterSet {
                                                    data: pps,
                                                }],
                                            },
                                        ),
                                    }],
                                }),
                            }],
                        }),
                    },
                    proto::BoxClass {
                        typ: "stts".into(),
                        value: BoxData::TimeToSampleBox(TimeToSampleBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            entries: stts_entries,
                        }),
                    },
                    proto::BoxClass {
                        typ: "stss".into(),
                        value: BoxData::SyncSampleBox(SyncSampleBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            sample_number: self.track.sample_key_indices.clone(),
                        }),
                    },
                    proto::BoxClass {
                        typ: "stsc".into(),
                        value: BoxData::SampleToChunkBox(SampleToChunkBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            entries: {
                                self.track
                                    .chunks
                                    .iter()
                                    .enumerate()
                                    .map(|(i, (_, num_samples))| SampleToChunkBoxEntry {
                                        first_chunk: (i + 1) as u32,
                                        samples_per_chunk: *num_samples as u32,
                                        sample_description_index: 1,
                                    })
                                    .collect()
                            },
                        }),
                    },
                    proto::BoxClass {
                        typ: "stsz".into(),
                        value: BoxData::SampleSizeBox(SampleSizeBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            sample_size: 0,
                            sample_count: num_samples,
                            sample_sizes: Some(self.track.sample_sizes.clone()),
                        }),
                    },
                    proto::BoxClass {
                        typ: "stco".into(),
                        value: BoxData::ChunkOffsetBox(ChunkOffsetBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            chunk_offsets: {
                                self.track
                                    .chunks
                                    .iter()
                                    .map(|(offset, _)| *offset as u32)
                                    .collect()
                            },
                        }),
                    },
                ],
            }),
        };

        let mut moov_children = vec![];
        moov_children.push(proto::BoxClass {
            typ: "mvhd".into(),
            value: BoxData::MovieHeaderBox(MovieHeaderBox {
                full_box_header: FullBoxHeader {
                    version: 0,
                    flags: 0,
                },
                v1: None,
                v0: Some(MovieHeaderBoxV0 {
                    creation_time: 0,
                    modification_time: 0,
                    timescale: MOVIE_TIMESCALE as u32,
                    duration: movie_duration,
                }),
                rate: 0x00010000, // 1.0x rate
                volume: 0x0100,   // 1.0 (full volume)
                matrix: [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000].into(),
                next_track_id: 2,
            }),
        });

        moov_children.push(proto::BoxClass {
            typ: "trak".into(),
            value: BoxData::TrackBox(ContainerBox {
                children: vec![
                    proto::BoxClass {
                        typ: "tkhd".into(),
                        value: BoxData::TrackHeaderBox(TrackHeaderBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 3, // track_enabled | track_in_movie
                            },
                            v1: None,
                            v0: Some(TrackHeaderBoxV0 {
                                creation_time: 0,
                                modification_time: 0,
                                track_id: 1,
                                duration: movie_duration,
                            }),
                            layer: 0,
                            alternate_group: 0,
                            volume: 0,
                            matrix: [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000].into(),
                            width: (self.frame_width << 16),
                            height: (self.frame_height << 16),
                        }),
                    },
                    // Skip 'edts'
                    proto::BoxClass {
                        typ: "mdia".into(),
                        value: BoxData::MediaBox(ContainerBox {
                            children: vec![
                                proto::BoxClass {
                                    typ: "mdhd".into(),
                                    value: BoxData::MediaHeaderBox(MediaHeaderBox {
                                        full_box_header: FullBoxHeader {
                                            version: 0,
                                            flags: 0,
                                        },
                                        v1: None,
                                        v0: Some(MovieHeaderBoxV0 {
                                            creation_time: 0,
                                            modification_time: 0,
                                            timescale: MEDIA_TIMESCALE as u32,
                                            duration: media_duration,
                                        }),
                                        language: 21956,
                                    }),
                                },
                                proto::BoxClass {
                                    typ: "hdlr".into(),
                                    value: BoxData::HandlerBox(HandlerBox {
                                        full_box_header: FullBoxHeader {
                                            version: 0,
                                            flags: 0,
                                        },
                                        handler_type: "vide".into(),
                                        name: "VideoHandler".into(),
                                    }),
                                },
                                proto::BoxClass {
                                    typ: "minf".into(),
                                    value: BoxData::MediaInformationBox(ContainerBox {
                                        children: vec![
                                            proto::BoxClass {
                                                typ: "vmhd".into(),
                                                value: BoxData::VideoMediaHeaderBox(
                                                    VideoMediaHeaderBox {
                                                        full_box_header: FullBoxHeader {
                                                            version: 0,
                                                            flags: 1,
                                                        },
                                                    },
                                                ),
                                            },
                                            dinf_box,
                                            stbl_box,
                                        ],
                                    }),
                                },
                            ],
                        }),
                    },
                ],
            }),
        });

        if self.options.fragment.is_some() {
            moov_children.push(BoxClass {
                typ: "mvex".into(),
                value: BoxData::MovieExtendsBox(ContainerBox {
                    children: vec![
                        BoxClass {
                            typ: "trex".into(),
                            value: BoxData::TrackExtendsBox(TrackExtendsBox {
                                full_box_header: FullBoxHeader {
                                    version: 0,
                                    flags: 0,
                                },
                                track_id: 1,
                                default_sample_description_index: 1, // TODO
                                default_sample_duration: media_sample_delta,
                                default_sample_size: 0,
                                default_sample_flags: 0, // TODO
                            }),
                        }, //
                    ],
                }),
            });
        }

        let mut out = vec![];
        proto::BoxClass {
            typ: "moov".into(),
            value: BoxData::MovieBox(ContainerBox {
                children: moov_children,
            }),
        }
        .serialize(&mut out)?;

        Ok(out)
    }

    fn append_nalu(data: &[u8], output_buffer: &mut Vec<u8>) {
        output_buffer.extend_from_slice(&(data.len() as u32).to_be_bytes());
        output_buffer.extend_from_slice(data);
    }

    /// Must be called either at the end of the file or before processing a key
    /// frame.
    fn maybe_finish_current_chunk(
        &mut self,
        next_keyframe_size: Option<usize>,
        next_frame_time: Duration,
        end_of_input: bool,
    ) -> Result<()> {
        // TODO: Think of about this more carefully.
        // No point in flushing empty chunks.
        if end_of_input && self.track.sample_sizes.is_empty() {
            return Ok(());
        }

        // 'ftyp' will always be in the first segment and maybe later segments if we
        // want them to be independent.
        if self.segment_file_offset + (self.segment_buffer.len() as u64) == 0
            && (self.segment_index == 0
                || self.options.independent_segments
                || self.options.fragment.is_none())
        {
            self.create_ftyp_box()?;
        }

        // First fragment contains the 'init' data (the 'moov')
        if self.options.fragment.is_some() && self.track.fragment_number == 0 {
            self.finish_init_fragment()?;
        }

        if self.track.sample_sizes.is_empty() {
            return Ok(());
        }

        // Check if the chunk is big enough to flush yet.
        let over_segment_size_limit = {
            if let Some(max_segment_size) = self.options.max_segment_size {
                self.segment_file_offset
                    + (self.segment_buffer.len() as u64)
                    + (next_keyframe_size.unwrap_or(0) as u64)
                    > max_segment_size
            } else {
                false
            }
        };
        {
            let over_fragment_size_limit = {
                if let Some(size) = self.options.fragment {
                    self.track.sample_sizes.len() > size
                } else {
                    false
                }
            };

            let over_chunk_size_limit = self.track.chunk_buffer.len()
                + next_keyframe_size.unwrap_or(0)
                > self.options.max_chunk_size;

            if !(over_chunk_size_limit
                || over_fragment_size_limit
                || over_segment_size_limit
                || end_of_input)
            {
                return Ok(());
            }
        }

        if self.options.fragment.is_some() {
            // Needed according to https://www.w3.org/2013/12/byte-stream-format-registry/isobmff-byte-stream-format.html
            proto::BoxClass {
                typ: "styp".into(),
                value: BoxData::SegmentTypeBox(FileTypeBox {
                    major_brand: "msdh".into(),
                    minor_version: 0,
                    compatible_brands: vec!["msdh".into(), "msix".into()],
                }),
            }
            .serialize(&mut self.segment_buffer)?;
        }

        let user_time_range = TimeRange {
            start: self.track.sample_times[self.track.media_base_num_samples],
            end: next_frame_time,
        };

        let start_time = self.base_time();

        let make_moof = |this: &Self,
                         data_offset: i32,
                         sample_durations: &[u32],
                         all_default_delta: bool| proto::BoxClass {
            typ: "moof".into(),
            value: BoxData::MovieFragmentBox(ContainerBox {
                children: vec![
                    BoxClass {
                        typ: "mfhd".into(),
                        value: BoxData::MovieFragmentHeaderBox(MovieFragmentHeaderBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            sequence_number: this.track.fragment_number as u32,
                        }),
                    },
                    BoxClass {
                        typ: "traf".into(),
                        value: BoxData::TrackFragmentBox(ContainerBox {
                            children: vec![
                                BoxClass {
                                    typ: "tfhd".into(),
                                    value: BoxData::TrackFragmentHeaderBox(
                                        TrackFragmentHeaderBox {
                                            full_box_header: FullBoxHeader {
                                                version: 0,
                                                flags: 0,
                                            },
                                            track_id: 1,
                                            // base_data_offset: None,
                                            // sample_description_index: None,
                                            // default_sample_duration: None,
                                            // default_sample_size: None,
                                            // default_sample_flags: None,
                                        },
                                    ),
                                },
                                BoxClass {
                                    typ: "tfdt".into(),
                                    value: BoxData::TrackFragmentBaseMediaDecodeTimeBox(
                                        TrackFragmentBaseMediaDecodeTimeBox {
                                            full_box_header: FullBoxHeader {
                                                version: 0,
                                                flags: 0,
                                            },
                                            // TODO: Use v1 for large videos?
                                            base_media_decode_time_v0: Some(
                                                this.track.media_base_time as u32,
                                            ),
                                            base_media_decode_time_v1: None,
                                        },
                                    ),
                                },
                                BoxClass {
                                    typ: "trun".into(),
                                    value: BoxData::TrackRunBox(TrackRunBox {
                                        full_box_header: FullBoxHeader {
                                            version: 0,
                                            flags: 0x1
                                                | (if all_default_delta { 0 } else { 0x000100 })
                                                | 0x000200, /* data-offset-present |
                                                             * sample-duration-present |
                                                             * sample-size-present
                                                             */
                                        },
                                        sample_count: (this.track.sample_sizes.len() as u32),
                                        data_offset: Some(data_offset),
                                        first_sample_flags: None,
                                        samples: this
                                            .track
                                            .sample_sizes
                                            .iter()
                                            .zip(sample_durations.iter())
                                            .map(|(sample_size, sample_duration)| {
                                                TrackRunBoxSample {
                                                    sample_duration: if all_default_delta {
                                                        None
                                                    } else {
                                                        Some(*sample_duration)
                                                    },
                                                    sample_size: Some(*sample_size),
                                                    sample_flags: None,
                                                    sample_composition_time_offset: None,
                                                }
                                            })
                                            .collect(),
                                    }),
                                },
                            ],
                        }),
                    },
                ],
            }),
        };

        if self.options.fragment.is_some() {
            let (sample_durations, media_time_delta) =
                Self::get_sample_durations(&self.track.sample_times, next_frame_time);

            // TODO: Run some duration compression/smoothing.

            // TODO: Dedup this line.
            let media_sample_delta = (MEDIA_TIMESCALE / (self.frame_rate as u64)) as u32;

            let all_default_delta = sample_durations
                .iter()
                .find(|v| **v != media_sample_delta)
                .is_none();

            // Data offset of the first byte in the 'mdat' data (relative to the start of
            // the 'moof')
            //
            // We add '+8' at account for the size of the 'mdat' box header that will added
            // in the next section.
            let data_offset = {
                let mut tmp = vec![];
                make_moof(self, 0, &sample_durations, all_default_delta).serialize(&mut tmp)?;
                tmp.len() as i32 + 8
            };

            make_moof(self, data_offset, &sample_durations, all_default_delta)
                .serialize(&mut self.segment_buffer)?;

            self.track.media_base_time += media_time_delta;
        }

        // Write the 'mdat'
        let data_absolute_offset;
        {
            proto::BoxHeader {
                typ: "mdat".into(),
                length: (self.track.chunk_buffer.len() + 8) as u32,
            }
            .serialize(&mut self.segment_buffer)?;

            // In non-fragmented mode, the sample offseta are relative to the start of the
            // file.
            data_absolute_offset = self.segment_file_offset + (self.segment_buffer.len() as u64);
            assert!(data_absolute_offset >= 0);

            self.segment_buffer
                .extend_from_slice(&self.track.chunk_buffer);
            self.track.chunk_buffer.clear();
        }

        if self.options.fragment.is_some() {
            self.track.sample_key_indices.clear();
            self.track.sample_sizes.clear();
            self.track.sample_times.clear();
            self.track.fragment_number += 1;

            // NOTE: media_base_time is incremented above while forming the
            // 'moof' since
        } else {
            self.track.chunks.push((
                data_absolute_offset as usize,
                self.track.sample_sizes.len() - self.track.media_base_num_samples,
            ));
            self.track.media_base_num_samples = self.track.sample_sizes.len();
            self.track.media_base_time =
                Self::to_media_time(user_time_range.end - self.track.sample_times[0]);
        }

        if end_of_input && self.options.fragment.is_none() {
            self.segment_buffer
                .extend_from_slice(&self.create_moov_box(next_frame_time)?);
        }

        let end_time = self.base_time();

        let data = self.take_output_buffer();
        self.output.push_back(MP4BuilderChunk {
            segment_index: self.segment_index,
            is_init: false,
            data,
            // TODO: Also need to factor in any skipped time due to skipped frames at the beginning
            // of the stream.
            time_range: Some(TimeRange {
                start: start_time,
                end: end_time,
            }),
            user_time_range: Some(user_time_range),
        });

        if over_segment_size_limit {
            self.segment_index += 1;
            self.segment_file_offset = 0;
            assert!(self.segment_buffer.is_empty());

            if self.options.fragment.is_none() || self.options.independent_segments {
                self.track = TrackState::default();
            }

            // May need to setup ftyp/moov boxes for the next segment.
            self.maybe_finish_current_chunk(next_keyframe_size, next_frame_time, end_of_input);
        }

        Ok(())
    }

    fn get_sample_durations(
        sample_times: &[Duration],
        next_frame_time: Duration,
    ) -> (Vec<u32>, u64) {
        let mut sample_durations = vec![];
        let mut media_time = 0;

        for i in 0..sample_times.len() {
            let next_sample_time = {
                if i + 1 < sample_times.len() {
                    sample_times[i + 1]
                } else {
                    next_frame_time
                }
            };

            // NOTE: We aren't considering any media time error compared to the last media
            // time of the last fragment.
            let next_media_time = Self::to_media_time(next_sample_time - sample_times[0]);

            let dur = next_media_time - media_time;
            sample_durations.push(dur as u32);

            media_time = next_media_time;
        }

        (sample_durations, media_time)
    }

    fn to_media_time(dur: Duration) -> u64 {
        // TODO: Use rounded division?
        (MEDIA_TIMESCALE * (dur.as_nanos() as u64)) / 1_000_000_000
    }

    fn base_time(&self) -> Duration {
        Duration::from_nanos((self.track.media_base_time * 1_000_000_000) / MEDIA_TIMESCALE)
    }

    /// Gets the next available chunk to be
    pub fn consume(&mut self) -> Option<MP4BuilderChunk> {
        self.output.pop_front()
    }
}
