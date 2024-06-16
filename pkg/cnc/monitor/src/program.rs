use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::{bytes::Bytes, io::Readable};
use executor::{
    bundle::TaskResultBundle,
    channel::{self, oneshot},
};
use file::{LocalFile, LocalPath};
use image::{format::jpeg::encoder::JPEGEncoder, types::ImageType, Image};

#[derive(Default, Debug)]
pub struct ProgramSummary {
    pub proto: ProgramSummaryProto,

    pub tools: HashMap<usize, ProgramToolSummary>,

    pub max_bed_temperature: Option<f32>,

    pub thumbnails: Vec<ProgramThumbnail>,

    pub unique_commands: HashSet<String>,
}

#[derive(Debug)]
pub struct ProgramThumbnail {
    pub data: Bytes,
    pub width: usize,
    pub height: usize,
}

#[derive(Default, Debug)]
pub struct ProgramToolSummary {
    pub max_extruder_temperature: Option<f32>,
}

impl ProgramSummary {
    pub async fn create(file_path: &LocalPath) -> Result<Self> {
        let mut bundle = TaskResultBundle::new();

        let (reader, chunks) = ChunkedFileReader::create(file_path).await?;
        bundle.add("ChunkedFileReader", reader.run());

        // TODO: Get rid of this since it doesn't enforce any line length limits and the
        // gcode parser can do its own line splitting now.
        let (splitter, lines) = LineSplitter::create(chunks)?;
        bundle.add("LineSplitter", splitter.run());

        let (summarizer, summary) = ProgramSummarizer::create(lines);
        bundle.add("ProgramSummarizer", summarizer.run());

        bundle.join().await?;

        summary
            .recv()
            .await
            .map_err(|_| err_msg("No summary for generated for an unknown reason"))
    }

    pub fn best_thumbnail(&self) -> Result<Option<Bytes>> {
        let mut best = None;
        let mut best_area = 0;
        let mut best_type = ImageType::BMP;

        for thumb in &self.thumbnails {
            let typ = match image::types::ImageType::from_header(&thumb.data) {
                Some(v) => v,
                None => continue,
            };

            let area = thumb.width * thumb.height;
            if area > best_area || (area == best_area && typ.widely_supported()) {
                best = Some(thumb.data.clone());
                best_area = area;
                best_type = typ;
            }
        }

        let mut data = match best {
            Some(v) => v,
            None => return Ok(None),
        };

        if !best_type.widely_supported() {
            let img = Image::<u8>::parse_from(&data)?;

            let mut out = vec![];
            JPEGEncoder::new(100).encode(&img, &mut out)?;
            data = out.into();
        }

        Ok(Some(data))
    }
}

pub struct ChunkedFileReader {
    file: LocalFile,
    sender: channel::Sender<Option<Bytes>>,
}

impl ChunkedFileReader {
    pub async fn create(file_path: &LocalPath) -> Result<(Self, channel::Receiver<Option<Bytes>>)> {
        let (sender, receiver) = channel::bounded(4);

        let file = LocalFile::open(file_path)?;

        let inst = Self { file, sender };

        Ok((inst, receiver))
    }

    pub async fn run(mut self) -> Result<()> {
        let mut file_size = self.file.metadata().await?.len();

        let mut offset = 0;
        self.file.seek(0);

        while offset < file_size {
            let n = core::cmp::min(file_size - offset, 8192) as usize;
            offset += n as u64;

            let mut data = vec![0u8; n];
            self.file.read_exact(&mut data).await?;

            if let Err(e) = self.sender.send(Some(data.into())).await {
                return Ok(());
            }
        }

        let _ = self.sender.send(None).await;
        Ok(())
    }
}

/// Emits lines with line endings.
pub struct LineSplitter {
    chunks: channel::Receiver<Option<Bytes>>,
    output: channel::Sender<Option<Bytes>>,
}

impl LineSplitter {
    pub fn create(
        chunks: channel::Receiver<Option<Bytes>>,
    ) -> Result<(Self, channel::Receiver<Option<Bytes>>)> {
        let (sender, receiver) = channel::bounded(16);
        let inst = Self {
            chunks,
            output: sender,
        };
        Ok((inst, receiver))
    }

    // TODO: If these fail, we need an entire an E-Stop mode.

    pub async fn run(mut self) -> Result<()> {
        let mut incomplete_line = vec![];

        loop {
            let mut chunk = match self.chunks.recv().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(_) => return Ok(()),
            };

            let mut i = 0;

            // TODO: Limit the max line size.
            loop {
                let j = chunk[i..].iter().position(|c| *c == b'\n');
                if let Some(j) = j {
                    let mut line = chunk.slice(i..(i + j + 1));
                    i = i + j + 1;

                    if !incomplete_line.is_empty() {
                        incomplete_line.extend_from_slice(&line);
                        line = incomplete_line.split_off(0).into();
                    }

                    if let Err(_) = self.output.send(Some(line)).await {
                        return Ok(());
                    }
                } else {
                    incomplete_line.extend_from_slice(&chunk[i..]);
                    break;
                }
            }
        }

        if !incomplete_line.is_empty() {
            let _ = self.output.send(Some(incomplete_line.into())).await;
        }

        let _ = self.output.send(None).await;
        Ok(())
    }
}

pub struct ProgramSummarizer {
    parser: gcode::Parser,
    lines: channel::Receiver<Option<Bytes>>,
    output: oneshot::Sender<ProgramSummary>,
    summary: ProgramSummary,
    partial_summary: PartialSummary,
}

#[derive(Default)]
struct PartialSummary {
    current_tool: usize,
    thumbnail: Option<PartialThumbnail>,
}

struct PartialThumbnail {
    start_tag: String,
    width: usize,
    height: usize,
    size: usize,
    data_base64: String,
}

impl ProgramSummarizer {
    pub fn create(
        lines: channel::Receiver<Option<Bytes>>,
    ) -> (Self, oneshot::Receiver<ProgramSummary>) {
        let (sender, receiver) = oneshot::channel();
        let mut inst = Self {
            parser: gcode::Parser::new(),
            lines,
            output: sender,
            summary: ProgramSummary::default(),
            partial_summary: PartialSummary::default(),
        };

        inst.summary.tools.insert(0, ProgramToolSummary::default());

        (inst, receiver)
    }

    pub async fn run(mut self) -> Result<()> {
        loop {
            // TODO: Need to parse the final line with end_of_input

            let line = match self.lines.recv().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(_) => return Ok(()),
            };

            *self.summary.proto.num_lines_mut() += 1;

            if let Err(e) = self.interpret_line(&line[..]) {
                eprintln!("{}", e);
                *self.summary.proto.num_invalid_lines_mut() += 1;
            }

            /*
            Gcodes to handle:

            G0, G1, G2, G3

            G20, G21 - set to inches, set to millis

            G28 - move to origin (home)

            G90 - absolute pos
            G91 - relative pos
            G92 - set position

            Good to check how many stops there are.
            M0 - stop
            M1 - sleep
            M2 - Program end
            M3, M4 - spindle on

            sleeps and stops we should handle internally since we don't want to block the serial bus

            M25 - pause SD card print



            */

            // Thumbnail

            // Requirements such as tool types and build volume.

            //
        }

        if self.partial_summary.thumbnail.is_some() {
            *self.summary.proto.num_invalid_lines_mut() += 1;
        }

        let _ = self.output.send(self.summary);

        Ok(())
    }

    fn interpret_line(&mut self, line: &[u8]) -> Result<()> {
        let mut builder = gcode::LineBuilder::new();

        let mut event_index = 0;
        {
            let mut error_in_line = false;
            let mut parser = gcode::Parser::new();
            let mut iter = parser.iter(&line[..], true);
            while let Some(e) = iter.next() {
                match e {
                    gcode::Event::LineNumber(_) => {}
                    gcode::Event::Comment(data, is_semi_comment) => {
                        if is_semi_comment && event_index == 0 {
                            let data = core::str::from_utf8(&data)?.trim();
                            if data.is_empty() {
                                return Ok(());
                            }

                            self.parse_thumbnail_comment(data)?;
                            return Ok(());
                        }
                    }
                    gcode::Event::ParseError(_) => {
                        return Err(err_msg("ParseError in line"));
                    }
                    gcode::Event::EndLine => {
                        // event_index = 0;
                    }
                    gcode::Event::Word(w) => {
                        builder.add_word(w)?;
                    }
                }

                event_index += 1;
            }
        }

        let line = match builder.finish() {
            Some(v) => v,
            None => return Ok(()),
        };

        self.summary
            .unique_commands
            .insert(line.command().to_string());

        if line.command().group == 'T' {
            // TODO: Verify that an integer was provided.
            let num = line.command().number.to_f32() as usize;

            self.summary.tools.entry(num).or_default();
            self.partial_summary.current_tool = num;
        }

        if line.command() == &gcode::Command::new('M', 73) {
            if let Some(v) = line.params().get(&'R') {
                if !self.summary.proto.has_normal_duration() {
                    self.summary
                        .proto
                        .set_normal_duration(Duration::from_secs_f32(v.to_f32()? * 60.0));
                };
            }

            if let Some(v) = line.params().get(&'S') {
                if !self.summary.proto.has_silent_duration() {
                    self.summary
                        .proto
                        .set_silent_duration(Duration::from_secs_f32(v.to_f32()? * 60.0));
                };
            }
        }

        // TODO: We should self implement any blocking gcodes in the player.
        if line.command() == &gcode::Command::new('M', 104)
            || line.command() == &gcode::Command::new('M', 109)
        {
            if let Some(temp) = line.params().get(&'S') {
                let t = self
                    .summary
                    .tools
                    .get_mut(&self.partial_summary.current_tool)
                    .unwrap();
                t.max_extruder_temperature = Some(f32::max(
                    t.max_extruder_temperature.unwrap_or(-10000.0),
                    temp.to_f32()?,
                ));
            }
        }

        // TODO: Also interpret the 'precise' temp parameters as well.
        if line.command() == &gcode::Command::new('M', 140)
            || line.command() == &gcode::Command::new('M', 190)
        {
            if let Some(temp) = line.params().get(&'S') {
                self.summary.max_bed_temperature = Some(f32::max(
                    self.summary.max_bed_temperature.unwrap_or(-10000.0),
                    temp.to_f32()?,
                ));
            }
        }

        if line.to_string_compact().len() > gcode::MAX_STANDARD_LINE_LENGTH {
            return Err(err_msg("Line is too long to send to machines"));
        }

        /*
        M83 ; extruder relative mode
        M104 S240 ; set extruder temp
        M140 S85 ; set bed temp
        M190 S85 ; wait for bed temp
        M109 S240 ; wait for extruder temp

        M191 <- chamber temperature

        */

        Ok(())
    }

    /*
    Typically thumbnails are stored in the gcode files with lines that look like the following:
    ;
    ; thumbnail begin 160x120 16996
    ; iVBORw0KGgoAAAANSUhEUgAAAKAAAAB4CAYAAAB1ovlvAAAxkUlEQVR4Ae2d+ZOc1Xnv33AL55Zvqs
    ; D32nHZufc6DoTYTqIYkxAMCNAyo32075p9RgsSWhBCbLIQq5DYDWYxqyWM2IwNGNsylRjjuJzEqSQ/
    ....
    ; pSr/zJv+nNG3eebM2d5WS5ZEfng06u63T5/le579PKcaHByse3p66uuuu24SrV27th4ZGalnzZpVz5
    ; thumbnail end
    ;
    */
    fn parse_thumbnail_comment(&mut self, data: &str) -> Result<()> {
        let parts = data.split_ascii_whitespace().collect::<Vec<_>>();

        if self.partial_summary.thumbnail.is_none()
            && (parts[0] == "thumbnail"
                || parts[0] == "thumbnail_QOI"
                || parts[0] == "thumbnail_JPG")
            && parts.len() >= 2
            && parts[1] == "begin"
        {
            if parts.len() < 4 {
                return Err(err_msg(
                    "Expected at least 4 fields in thumbnail start line",
                ));
            }

            let (width_str, height_str) = parts[2]
                .split_once('x')
                .ok_or_else(|| err_msg("Invalid image dimensions format"))?;
            let width = width_str.parse::<usize>()?;
            let height = height_str.parse::<usize>()?;

            let size = parts[3].parse::<usize>()?;

            self.partial_summary.thumbnail = Some(PartialThumbnail {
                start_tag: parts[0].to_string(),
                width,
                height,
                size,
                data_base64: String::new(),
            });
            return Ok(());
        }

        let mut thumb = match self.partial_summary.thumbnail.take() {
            Some(v) => v,
            None => return Ok(()),
        };

        if parts.len() >= 2 && parts[0] == &thumb.start_tag && parts[1] == "end" {
            if thumb.data_base64.len() != thumb.size {
                return Err(err_msg("Not enough data was parsed for the thumbnail"));
            }

            let data = base_radix::base64_decode(&thumb.data_base64)?.into();

            self.summary.thumbnails.push(ProgramThumbnail {
                data,
                width: thumb.width,
                height: thumb.height,
            });
            return Ok(());
        }

        // TODO: Don't allow overflowing the size.
        thumb.data_base64.push_str(data);

        self.partial_summary.thumbnail = Some(thumb);

        Ok(())
    }
}
