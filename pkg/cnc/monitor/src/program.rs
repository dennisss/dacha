use std::f32::consts::PI;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::hash::FastHasherBuilder;
use common::{bytes::Bytes, io::Readable};
use executor::{
    bundle::TaskResultBundle,
    channel::{self, oneshot, spsc},
    sync::SyncMutex,
};
use file::{LocalFile, LocalPath};
use gcode::CommandCodec;
use image::{format::jpeg::encoder::JPEGEncoder, types::ImageType, Image};
use math::matrix::cwise_binary_ops::{CwiseMax, CwiseMin};
use math::matrix::{Vector2f, Vector3f};
use reflection::ParseFrom;

use crate::{program_preview::*, round_number};

pub struct ProgressSender {
    state: Arc<SyncMutex<f32>>,
    sender: spsc::Sender<()>,
}

impl ProgressSender {
    pub fn update(&mut self, progress: f32) {
        self.state
            .apply(|v| {
                *v = progress;
            })
            .unwrap();
        let _ = self.sender.try_send(());
    }
}

pub struct ProgressReceiver {
    state: Arc<SyncMutex<f32>>,
    receiver: spsc::Receiver<()>,
}

impl ProgressReceiver {
    pub fn current(&self) -> f32 {
        self.state.read().unwrap()
    }

    /// When this returns None, it means that there will be no more updates.
    pub async fn wait(&mut self) -> Option<f32> {
        match self.receiver.recv().await {
            Ok(()) => Some(self.current()),
            Err(_) => None,
        }
    }
}

/// TODO: Generalize this pattern since we tend to have bounded(1) channels
/// fairly frequently.
pub fn new_progress_tracker() -> (ProgressSender, ProgressReceiver) {
    let (sender, receiver) = spsc::bounded(1);

    let state = Arc::new(SyncMutex::new(0.0));

    (
        ProgressSender {
            state: state.clone(),
            sender,
        },
        ProgressReceiver { state, receiver },
    )
}

#[derive(Default, Debug)]
pub struct ProgramSummary {
    pub proto: ProgramSummaryProto,

    pub tools: HashMap<usize, ProgramToolSummary>,

    pub max_bed_temperature: Option<f32>,

    pub thumbnails: Vec<gcode::ProgramThumbnail>,

    pub unique_commands: HashSet<String>,
}

#[derive(Default, Debug)]
pub struct ProgramToolSummary {
    pub max_extruder_temperature: Option<f32>,
}

impl ProgramSummary {
    pub async fn create(
        file_path: &LocalPath,
        file_size: u64,
        progress_reporter: ProgressSender,
    ) -> Result<Self> {
        let mut bundle = TaskResultBundle::new();

        let (reader, chunks) = ChunkedFileReader::create(file_path).await?;
        bundle.add("ChunkedFileReader", reader.run());

        let (mut parser, lines) = ProgramParserOp::new(chunks);
        parser.set_progress_reporter(file_size, progress_reporter);
        bundle.add("ProgramParser", parser.run());

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

/// Parses a program. The output will be a program elements grouped by program
/// line.
///
/// - ProgramElement::Error will never be emitted.
/// - ProgramElement::EndOfLine will never be emitted.
pub struct ProgramParserOp {
    chunks: channel::Receiver<Option<Bytes>>,
    output: channel::Sender<Option<Vec<gcode::ProgramElement>>>,

    elements: Vec<gcode::ProgramElement>,
    current_line: Option<Vec<gcode::ProgramElement>>,

    // first element is the file size.
    progress_reporter: Option<(u64, ProgressSender)>,
}

impl ProgramParserOp {
    pub fn new(
        chunks: channel::Receiver<Option<Bytes>>,
    ) -> (Self, channel::Receiver<Option<Vec<gcode::ProgramElement>>>) {
        let (sender, receiver) = channel::bounded(20);

        let inst = Self {
            chunks,
            output: sender,
            elements: vec![],
            current_line: None,
            progress_reporter: None,
        };

        (inst, receiver)
    }

    pub fn set_progress_reporter(&mut self, file_size: u64, sender: ProgressSender) {
        self.progress_reporter = Some((file_size, sender));
    }

    pub async fn run(mut self) -> Result<()> {
        let mut parser = bgcode::ProgramParser::default();

        let mut total_bytes_processed = 0;

        loop {
            let mut chunk = match self.chunks.recv().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(_) => return Ok(()),
            };

            let mut remaining = &chunk[..];
            while !remaining.is_empty() {
                let n = parser.parse_line(remaining, false, &mut self.elements)?;
                if n == 0 {
                    return Err(err_msg("GCode program parser failed to advance."));
                }

                remaining = &remaining[n..];

                self.process_elements().await?;

                total_bytes_processed += n;

                if let Some((file_size, progress_reporter)) = &mut self.progress_reporter {
                    let progress = ((total_bytes_processed as f64) / (*file_size as f64)) as f32;
                    progress_reporter.update(progress);
                }
            }
        }

        let _ = parser.parse_line(&[], true, &mut self.elements);
        self.process_elements().await?;

        let _ = self.output.send(None).await;

        Ok(())
    }

    async fn process_elements(&mut self) -> Result<()> {
        for element in self.elements.drain(..) {
            match element {
                gcode::ProgramElement::Error(e) => return Err(e),
                gcode::ProgramElement::EndOfLine => {
                    let line = match self.current_line.take() {
                        Some(v) => v,
                        None => vec![],
                    };

                    if let Err(_) = self.output.send(Some(line)).await {
                        return Ok(());
                    }
                }
                el => {
                    self.current_line.get_or_insert_with(|| vec![]).push(el);
                }
            }
        }

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
    parser: gcode::ProgramParser,
    lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
    output: oneshot::Sender<ProgramSummary>,
    summary: ProgramSummary,
    partial_summary: PartialSummary,
}

#[derive(Default)]
struct PartialSummary {
    current_tool: usize,
    current_position: Vector3f,
    current_coordinate_system: String,
    current_object: i32,
    bounds: HashMap<String, Bounds, FastHasherBuilder>,
    objects: HashMap<u32, ObjectData, FastHasherBuilder>,
}

#[derive(Default)]
struct ObjectData {
    proto: ProgramObjectSummary,
    min_position: Option<Vector3f>,
    max_position: Option<Vector3f>,
}

#[derive(Default)]
struct Bounds {
    min_position: Option<Vector3f>,
    max_position: Option<Vector3f>,
}

fn vector_to_proto(v: &Vector3f) -> Vector3fProto {
    let mut out = Vector3fProto::default();
    out.set_x(round_number(v[0]));
    out.set_y(round_number(v[1]));
    out.set_z(round_number(v[2]));
    out
}

impl ProgramSummarizer {
    pub fn create(
        lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
    ) -> (Self, oneshot::Receiver<ProgramSummary>) {
        let (sender, receiver) = oneshot::channel();
        let mut inst = Self {
            parser: gcode::ProgramParser::default(),
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
                // Done all lines.
                Ok(None) => break,
                // Input stream broken.
                Err(_) => return Ok(()),
            };

            *self.summary.proto.num_lines_mut() += 1;

            if let Err(e) = self.interpret_line(line) {
                if self.summary.proto.first_failures_len() < 5 {
                    let l = self.summary.proto.num_lines();
                    self.summary
                        .proto
                        .add_first_failures(format!("Line {}: {}", l, e));
                }

                *self.summary.proto.num_invalid_lines_mut() += 1;
            }

            // TODO: Need to ideally run through the same code as the
            // Player/FakeMachine to ensure that we can handle all gcodes.

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
        }

        for (coordinate_system, bounds) in self.partial_summary.bounds {
            let proto = self.summary.proto.new_bounds();
            proto.set_coordinate_system(coordinate_system);
            if let Some(pos) = &bounds.min_position {
                proto.set_min_position(vector_to_proto(pos));
            }
            if let Some(pos) = &bounds.max_position {
                proto.set_max_position(vector_to_proto(pos));
            }
        }

        // Add the objects in order of index. There should be no missing indices.
        for i in 0..self.partial_summary.objects.len() {
            let object = self
                .partial_summary
                .objects
                .get(&(i as u32))
                .ok_or_else(|| format_err!("Missing object with index: {}", i))?;

            let proto = self.summary.proto.add_objects(object.proto.clone());
            proto.set_index(i as u32);
            if let Some(pos) = &object.min_position {
                proto.set_min_position(vector_to_proto(pos));
            }
            if let Some(pos) = &object.max_position {
                proto.set_max_position(vector_to_proto(pos));
            }
        }

        let _ = self.output.send(self.summary);

        Ok(())
    }

    fn interpret_line(&mut self, elements: Vec<gcode::ProgramElement>) -> Result<()> {
        let mut out_line = gcode::LineBuilder::default();

        /*
        TODO: Measure how much is extruded from each tool.

        */

        // TODO: Elements need to be sorted so that things like spindle turn ons happen
        // before movements.
        for element in elements {
            let command = match element {
                gcode::ProgramElement::Thumbnail(thumbnail) => {
                    self.summary.thumbnails.push(thumbnail);
                    continue;
                }
                gcode::ProgramElement::Command(c) => c,
                gcode::ProgramElement::Metadata { key, value } => {
                    self.interpret_metadata(key, value)?;
                    continue;
                }

                _ => continue,
            };

            /*
            ; objects_info = {"objects":[{"name":"xyzCalibration_cube.stl (Instance 1)","polygon":[[154.024,151.627],[134.024,151.627],[134.024,131.627],[154.024,131.627]]},{"name":"xyzCalibration_cube.stl (Instance 2)","polygon":[[175.918,230.018],[155.918,230.018],[155.918,210.018],[175.918,210.018]]},{"name":"xyzCalibration_cube.stl (Instance 3)","polygon":[[255.894,150.182],[235.894,150.182],[235.894,130.182],[255.894,130.182]]}]}

            M486 S0
            M486 AxyzCalibration_cube.stl (Instance 1)
            M486 S-1
            M486 S1
            M486 AxyzCalibration_cube.stl (Instance 2)
            M486 S-1
            M486 S2
            M486 AxyzCalibration_cube.stl (Instance 3)
            M486 S-1

            ...

            M486 S2

            M486 S-1
            M486 S0

            */

            match &command {
                gcode::Command::Workspace1Coordinates(_) => {
                    self.partial_summary.current_coordinate_system = "G54".into();
                    // TODO: If the coordinate system changed, clear the current
                    // position.
                }
                gcode::Command::CancelObject(cmd) => {
                    if let Some(idx) = cmd.starting_object_index {
                        self.partial_summary.current_object = idx;
                    }

                    if self.partial_summary.current_object >= 0 {
                        let idx = self.partial_summary.current_object as u32;

                        let obj = self.partial_summary.objects.entry(idx).or_default();

                        if let Some(name) = &cmd.object_name {
                            obj.proto.set_name(name.clone());
                        }
                    } else if let Some(name) = &cmd.object_name {
                        // TODO: In RepRapFirmware, it is not necessary to specify the index. As
                        // long as there is a name, that will uniquely
                        // identify the objects.
                        return Err(err_msg(
                            "Object name given when no object index is selected",
                        ));
                    }
                }
                gcode::Command::LinearMove(gcode::LinearMove { inner })
                | gcode::Command::RapidMove(gcode::RapidMove { inner }) => {
                    // TODO: Ideally re-use the FakeMachine code for simulating motions. Currently
                    // this code assumptions that everything is using absolute coordinates.
                    let mut new_position = self.partial_summary.current_position.clone();

                    for (i, value) in [inner.x, inner.y, inner.z].into_iter().enumerate() {
                        if let Some(v) = value {
                            new_position[i] = v.to_f32();
                        }
                    }

                    self.partial_summary.current_position = new_position.clone();

                    let bounds = self
                        .partial_summary
                        .bounds
                        .entry(self.partial_summary.current_coordinate_system.clone())
                        .or_insert_with(|| Bounds::default());

                    bounds.min_position =
                        Some(new_position.clone().cwise_min(
                            bounds.min_position.clone().unwrap_or(new_position.clone()),
                        ));

                    bounds.max_position =
                        Some(new_position.clone().cwise_max(
                            bounds.max_position.clone().unwrap_or(new_position.clone()),
                        ));

                    if self.partial_summary.current_object >= 0 {
                        let object = self
                            .partial_summary
                            .objects
                            .get_mut(&(self.partial_summary.current_object as u32))
                            .ok_or_else(|| err_msg("Missing current object data"))?;

                        object.min_position = Some(new_position.clone().cwise_min(
                            object.min_position.clone().unwrap_or(new_position.clone()),
                        ));

                        object.max_position = Some(new_position.clone().cwise_max(
                            object.max_position.clone().unwrap_or(new_position.clone()),
                        ));
                    }
                }
                gcode::Command::SetBuildPercentage(cmd) => {
                    if let Some(v) = cmd.normal_time_remaining_mins {
                        if !self.summary.proto.has_normal_duration() {
                            self.summary
                                .proto
                                .set_normal_duration(Duration::from_secs_f32(v.to_f32() * 60.0));
                        }
                    }

                    if let Some(v) = cmd.silent_time_remaining_mins {
                        if !self.summary.proto.has_silent_duration() {
                            self.summary
                                .proto
                                .set_silent_duration(Duration::from_secs_f32(v.to_f32() * 60.0));
                        }
                    }
                }

                gcode::Command::SetExtruderTemperature(gcode::SetExtruderTemperature { inner })
                | gcode::Command::SetExtruderTemperatureAndWait(
                    gcode::SetExtruderTemperatureAndWait { inner },
                ) => {
                    let mut tool = self.partial_summary.current_tool;
                    if let Some(t) = inner.tool {
                        if t < 0 {
                            return Err(err_msg("Tool index < 0"));
                        }

                        tool = t as usize;
                    }

                    let temp = inner
                        .target_temperature
                        .or(inner.min_temperature)
                        .ok_or_else(|| err_msg("Missing temperature parameter"))?;

                    let t = self.summary.tools.entry(tool).or_default();
                    t.max_extruder_temperature = Some(f32::max(
                        t.max_extruder_temperature.unwrap_or(-10000.0),
                        temp.to_f32(),
                    ));
                }
                gcode::Command::SetBedTemperature(gcode::SetBedTemperature { inner })
                | gcode::Command::SetBedTemperatureAndWaitCommand(
                    gcode::SetBedTemperatureAndWaitCommand { inner },
                ) => {
                    let temp = inner
                        .target_temperature
                        .or(inner.min_temperature)
                        .ok_or_else(|| err_msg("Missing temperature parameter"))?;

                    // let old_max = self.summary.proto.has_max_bed_temperature

                    // let v = f32::max(
                    //     if self.summary.proto.has_max_bed_temperature() {  }
                    // self.summary.max_bed_temperature.unwrap_or(-10000.0),
                    //     temp.to_f32(),
                    // )

                    self.summary.max_bed_temperature = Some(f32::max(
                        self.summary.max_bed_temperature.unwrap_or(-10000.0),
                        temp.to_f32(),
                    ));
                }
                gcode::Command::ToolChange(cmd) => {
                    let num = cmd.tool as usize;
                    self.summary.tools.entry(num).or_default();
                    self.partial_summary.current_tool = num;
                }
                gcode::Command::SelectTool(cmd) => {
                    let num = cmd.index as usize;
                    self.summary.tools.entry(num).or_default();
                    self.partial_summary.current_tool = num;
                }
                _ => {}
            };

            let command_word = {
                let mut words = vec![];
                command.to_command_words(&mut words);
                words[0].to_string()
            };

            self.summary.unique_commands.insert(command_word);

            out_line.add(&command);
        }

        // TODO: Move this to the PlayerPreprocessor.
        // TODO: gRBL limit is 128?
        if out_line.to_string_compact().len() > gcode::MAX_STANDARD_LINE_LENGTH {
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

    fn interpret_metadata(&mut self, key: String, value: String) -> Result<()> {
        if key == "objects_info" {
            let value = json::parse(&value)?;
            let info = gcode::ObjectsInfo::parse_from(json::ValueParser::new(&value))?;

            for (i, object) in info.objects.into_iter().enumerate() {
                let entry = self.partial_summary.objects.entry(i as u32).or_default();

                if entry.proto.name().is_empty() {
                    entry.proto.set_name(object.name);
                }

                if entry.proto.polygon_len() != 0 {
                    return Err(err_msg("Object polygon defined multiple times"));
                }

                for pt in object.polygon {
                    if pt.len() != 2 {
                        return Err(err_msg("Expected polygon points to be 2d"));
                    }

                    let out = entry.proto.new_polygon();
                    out.set_x(pt[0]);
                    out.set_y(pt[1]);
                }
            }
        }

        //

        Ok(())
    }
}
