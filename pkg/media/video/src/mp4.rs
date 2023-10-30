use common::errors::*;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/src/mp4.rs"));

    impl BoxClass {
        pub fn children(&self) -> &[BoxClass] {
            match &self.value {
                BoxData::FileTypeBox(_)
                | BoxData::MovieHeaderBox(_)
                | BoxData::TrackHeaderBox(_)
                | BoxData::MediaHeaderBox(_)
                | BoxData::HandlerBox(_)
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
                | BoxData::MovieFragmentBox(v)
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
                | BoxData::MovieHeaderBox(_)
                | BoxData::TrackHeaderBox(_)
                | BoxData::MediaHeaderBox(_)
                | BoxData::HandlerBox(_)
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
                | BoxData::MovieFragmentBox(v)
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

pub struct MP4Builder {
    frame_width: u32,
    frame_height: u32,
    frame_rate: u32,

    params: Parameters,

    /// TODO: It would be better to make this a rope so that we don't need to
    /// re-allocate the buffer continously.
    output_buffer: Vec<u8>,

    /// Offset in the file where the first sample starts (first byte of data in
    /// the 'mdat' box).
    first_sample_offset: usize,

    /// Size of each sample in the file.
    ///
    /// - Each sample is 1 or more H264 NALUs with 4-byte length prefixes.
    /// - The first sample includes 3 NALUs (PPS + SPS + IDR frame).
    /// - Other samples are normally just one NALU picture frame.
    sample_sizes: Vec<u32>,

    /// Sorted list of sample indices which contain key/I-frames.
    ///
    /// NOTE: Sample indexes start at 1 (not 0).
    sample_key_indices: Vec<u32>,
}

#[derive(Default, Clone)]
struct Parameters {
    sequence_parameter_set: Option<Vec<u8>>,
    picture_parameter_set: Option<Vec<u8>>,
    target_file_size: Option<usize>,
}

impl MP4Builder {
    /// Creates a new empty instance for building an MP4.
    ///
    /// - frame_rate: frames per second in the video track.
    pub fn new(frame_width: u32, frame_height: u32, frame_rate: u32) -> Result<Self> {
        let mut output_buffer = vec![];

        proto::BoxClass {
            typ: "ftyp".into(),
            value: BoxData::FileTypeBox(FileTypeBox {
                major_brand: "isom".into(),
                minor_version: 512,
                compatible_brands: vec!["isom".into(), "iso2".into(), "avc1".into(), "mp41".into()],
            }),
        }
        .serialize(&mut output_buffer)?;

        proto::BoxHeader {
            typ: "mdat".into(),
            length: 0,
        }
        .serialize(&mut output_buffer)?;

        let first_sample_offset = output_buffer.len();

        Ok(Self {
            frame_height,
            frame_width,
            frame_rate,
            params: Parameters::default(),
            output_buffer,
            first_sample_offset,
            sample_sizes: vec![],
            sample_key_indices: vec![],
        })
    }

    /// Sets a target for the total size of the generated MP4 file.
    ///
    /// Once we have buffered more than this number of bytes, we will try to
    /// stop accepting more bytes. Note that we will only stop accepting bytes
    /// once we hit an I-frame boundary so that the next file can start with a
    /// complete frame.
    pub fn set_target_file_size(&mut self, size: usize) {
        self.params.target_file_size = Some(size);
        // TODO: reserve bytes?
    }

    /// Appends bytes of a raw H264 with start codes stream to the MP4. The data
    /// is buffered in memory until the MP4 is fully built.
    ///
    /// Returns the number of bytes of the stream that were consumed. If not all
    /// bytes were consumed, then the builder has decided to end the file early
    /// to hit a user configured threshold.
    pub fn append(&mut self, h264_stream: &[u8]) -> Result<usize> {
        let mut iter = H264BitStreamIterator::new(h264_stream);

        while let Some(nalu) = iter.peek() {
            let (header, rest) = NALUnitHeader::parse(nalu.data())?;
            match header.nal_unit_type {
                NALUnitType::PPS => {
                    if self.params.picture_parameter_set.is_some() {
                        return Err(err_msg("Having multiple PSP NALUs is not supported"));
                    }

                    self.params.picture_parameter_set = Some(nalu.data().to_vec());
                }
                NALUnitType::SPS => {
                    if self.params.sequence_parameter_set.is_some() {
                        return Err(err_msg("Having multiple SPS NALUs is not supported"));
                    }

                    self.params.sequence_parameter_set = Some(nalu.data().to_vec());
                }
                NALUnitType::IDRPicture => {
                    if !self.sample_sizes.is_empty() && let Some(target_size) = self.params.target_file_size {
                        let estimated_size = self.output_buffer.len() + (self.sample_sizes.len() * 4) + 100;
                        if estimated_size >= target_size {
                            break;
                        }
                    }

                    let start_size = self.output_buffer.len();

                    if self.sample_sizes.is_empty() {
                        let sps = self
                            .params
                            .sequence_parameter_set
                            .as_ref()
                            .ok_or_else(|| err_msg("Expected SPS data before first frame"))?;
                        let pps = self
                            .params
                            .picture_parameter_set
                            .as_ref()
                            .ok_or_else(|| err_msg("Expected PPS data before first frame"))?;

                        Self::append_nalu(&sps, &mut self.output_buffer);
                        Self::append_nalu(&pps, &mut self.output_buffer);
                    }

                    Self::append_nalu(nalu.data(), &mut self.output_buffer);

                    let end_size = self.output_buffer.len();
                    self.sample_sizes.push((end_size - start_size) as u32);
                    self.sample_key_indices.push(self.sample_sizes.len() as u32);
                }
                NALUnitType::NonIDRPicture => {
                    if self.sample_sizes.is_empty() {
                        return Err(err_msg(
                            "Expected the first sample to contain a IDR picture",
                        ));
                    }

                    let start_size = self.output_buffer.len();

                    Self::append_nalu(nalu.data(), &mut self.output_buffer);

                    let end_size = self.output_buffer.len();
                    self.sample_sizes.push((end_size - start_size) as u32);
                }
                v @ _ => {
                    return Err(format_err!("Unsupported NALU type: {:?}", v));
                }
            }

            nalu.advance();
        }

        let n = h264_stream.len() - iter.remaining().len();
        Ok(n)
    }

    fn append_nalu(data: &[u8], output_buffer: &mut Vec<u8>) {
        output_buffer.extend_from_slice(&(data.len() as u32).to_be_bytes());
        output_buffer.extend_from_slice(data);
    }

    /// Finishes building this MP4 and returns all the accumulated data.
    pub fn finish(mut self) -> Result<Vec<u8>> {
        // Set the correct length for the 'mdat' box now that it is complete.
        // TODO: Re-use the BoxHeader data structure for this?
        let mdat_length = (self.output_buffer.len() - self.first_sample_offset + 8) as u32;
        self.output_buffer[(self.first_sample_offset - 8)..(self.first_sample_offset - 4)]
            .copy_from_slice(&mdat_length.to_be_bytes());

        let num_samples = self.sample_sizes.len() as u32;

        let sps = self
            .params
            .sequence_parameter_set
            .take()
            .ok_or_else(|| err_msg("Empty video: Missing SPS"))?;
        let pps = self
            .params
            .picture_parameter_set
            .take()
            .ok_or_else(|| err_msg("Empty video: Missing SPS"))?;

        // Parsing critical bytes in the SPS
        if sps.len() < 3 {
            return Err(err_msg("SPS is too small"));
        }
        let profile_idc = sps[0];
        let profile_compatibility = sps[1];
        let level_idc = sps[2];

        let movie_timescale = 1000;

        // TODO: Use 64-bit precision for this calculation.
        let movie_duration = (num_samples * movie_timescale) / self.frame_rate;

        let media_timescale = 1200000;
        let media_sample_delta = media_timescale / self.frame_rate;
        // TODO: Use 64-bit precision for this calculation.
        let media_duration = (num_samples * media_timescale) / self.frame_rate;

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
                            entries: vec![TimeToSampleBoxEntry {
                                sample_count: self.sample_sizes.len() as u32,
                                sample_delta: media_sample_delta,
                            }],
                        }),
                    },
                    proto::BoxClass {
                        typ: "stss".into(),
                        value: BoxData::SyncSampleBox(SyncSampleBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            sample_number: self.sample_key_indices,
                        }),
                    },
                    proto::BoxClass {
                        typ: "stsc".into(),
                        value: BoxData::SampleToChunkBox(SampleToChunkBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            entries: vec![
                                // TODO: Should we define multiple chunks?
                                SampleToChunkBoxEntry {
                                    first_chunk: 1,
                                    samples_per_chunk: num_samples,
                                    sample_description_index: 1,
                                },
                            ],
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
                            sample_sizes: Some(self.sample_sizes),
                        }),
                    },
                    proto::BoxClass {
                        typ: "stco".into(),
                        value: BoxData::ChunkOffsetBox(ChunkOffsetBox {
                            full_box_header: FullBoxHeader {
                                version: 0,
                                flags: 0,
                            },
                            chunk_offsets: vec![self.first_sample_offset as u32],
                        }),
                    },
                ],
            }),
        };

        proto::BoxClass {
            typ: "moov".into(),
            value: BoxData::MovieBox(ContainerBox {
                children: vec![
                    proto::BoxClass {
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
                                timescale: movie_timescale,
                                duration: movie_duration,
                            }),
                            rate: 0x00010000, // 1.0x rate
                            volume: 0x0100,   // 1.0 (full volume)
                            matrix: [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000].into(),
                            next_track_id: 2,
                        }),
                    },
                    proto::BoxClass {
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
                                        matrix: [
                                            0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000,
                                        ]
                                        .into(),
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
                                                        timescale: media_timescale,
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
                                                                    full_box_header:
                                                                        FullBoxHeader {
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
                    },
                ],
            }),
        }
        .serialize(&mut self.output_buffer)?;

        Ok(self.output_buffer)
    }

    /// Finishes the current segment and returns a new builder that can be used
    /// to continue appending frames from the same H264 stream.
    ///
    /// Note that appending the next segment will fail unless we are at an
    /// I-frame boundary in the stream (append() didn't consume all data in the
    /// last call).
    ///
    /// Returns (current_segment_mp4, next_segment_builder)
    pub fn finish_segment(mut self) -> Result<(Vec<u8>, Self)> {
        let mut next_segment = Self::new(self.frame_width, self.frame_height, self.frame_rate)?;
        // TODO: Re-use this without copies.
        next_segment.params = self.params.clone();

        let current_segment = self.finish()?;
        Ok((current_segment, next_segment))
    }
}
