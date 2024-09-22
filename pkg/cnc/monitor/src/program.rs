use std::f32::consts::PI;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::{bytes::Bytes, io::Readable};
use executor::{
    bundle::TaskResultBundle,
    channel::{self, oneshot, spsc},
    sync::SyncMutex,
};
use file::{LocalFile, LocalPath};
use gcode::CommandCodec;
use graphics::canvas::{Canvas, Paint, PathBuilder};
use image::Color;
use image::{format::jpeg::encoder::JPEGEncoder, types::ImageType, Image};
use math::matrix::cwise_binary_ops::{CwiseMax, CwiseMin};
use math::matrix::{Vector2f, Vector3f};

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

    pub async fn visualize(
        file_path: &LocalPath,
        summary: &ProgramSummaryProto,
    ) -> Result<ProgramVisualization> {
        let mut bundle = TaskResultBundle::new();

        let (reader, chunks) = ChunkedFileReader::create(file_path).await?;
        bundle.add("ChunkedFileReader", reader.run());

        let (parser, lines) = ProgramParserOp::new(chunks);
        bundle.add("ProgramParser", parser.run());

        let (visualizer, visual) = ProgramVisualizer::create(summary, lines)?;
        bundle.add("ProgramVisualizer", visualizer.run());

        bundle.join().await?;

        visual
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
    min_position: Option<Vector3f>,
    max_position: Option<Vector3f>,
}

fn vector_to_proto(v: &Vector3f) -> Vector3fProto {
    let mut out = Vector3fProto::default();
    out.set_x(v[0]);
    out.set_y(v[1]);
    out.set_z(v[2]);
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

        if let Some(pos) = &self.partial_summary.min_position {
            self.summary.proto.set_min_position(vector_to_proto(pos));
        }

        if let Some(pos) = &self.partial_summary.max_position {
            self.summary.proto.set_max_position(vector_to_proto(pos));
        }

        let _ = self.output.send(self.summary);

        Ok(())
    }

    fn interpret_line(&mut self, elements: Vec<gcode::ProgramElement>) -> Result<()> {
        let mut out_line = gcode::LineBuilder::default();

        /*
        TODO: Dimensions along which to split the bounding boxes:
        - Overall X/Y/Z bounds.
        - Per-object bounds.

        Tracing the motion:
        - Assumptions:
            - Not using any position offsets like G92 within the program.
            - Using all absolute movements

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
                _ => continue,
            };

            match &command {
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

                    self.partial_summary.min_position = Some(
                        new_position.clone().cwise_min(
                            self.partial_summary
                                .min_position
                                .clone()
                                .unwrap_or(new_position.clone()),
                        ),
                    );

                    self.partial_summary.max_position = Some(
                        new_position.clone().cwise_max(
                            self.partial_summary
                                .max_position
                                .clone()
                                .unwrap_or(new_position.clone()),
                        ),
                    );
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
}

/*
- For CNC, color in everything with Z < 0

- Generate one image per tool
- For 3d printing, also do one image per layer
    - Highlight overhang in another color

*/

/// TODO: Base this on the largest tool diameter.
const MARGIN_MM: f32 = 5.0;

const PIXELS_PER_MM: f32 = 10.0;

fn encode_binary_image(image: &Image<u8>) -> Vec<u8> {
    let mut out = vec![];

    out.extend_from_slice(&(image.height() as u32).to_le_bytes());
    out.extend_from_slice(&(image.width() as u32).to_le_bytes());

    for y in 0..image.height() {
        for x in 0..image.width() {
            if x % 8 == 0 {
                out.push(0);
            }

            if image[(y, x, 0)] != 0 {
                *out.last_mut().unwrap() |= 1 << (x % 8);
            }
        }
    }

    let mut compressed = vec![];

    let comper = compression::deflate::Deflater::new();
    compression::transform::transform_to_vec(comper, &out, &mut compressed).unwrap();

    compressed
}

pub struct ProgramVisualization {
    pub image: Vec<u8>,
    pub packed: Vec<u8>,
}

pub struct ProgramVisualizer {
    summary: ProgramSummaryProto,
    lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
    parser: gcode::ProgramParser,
    canvas: graphics::raster::canvas::RasterCanvas,
    output: oneshot::Sender<ProgramVisualization>,
}

/*
We can generate visualizations for specific machines.
- Note that if the config changes, then we need to re-compute things though.

-

Obviously the simplest option is to just always re-compute everything.


TODO: Eventually we will want to save things like the tool configs in history reports since that is relevant for analysis.

*/

fn calculate_engraving_diameter(angle_degrees: f32, base_diameter: f32, depth: f32) -> f32 {
    let angle_rads = angle_degrees * (PI / 180.0);
    let half_angle_rads = angle_rads / 2.0;

    let tan = half_angle_rads.tan();

    let base_depth = (base_diameter / 2.0) / tan;

    let full_depth = base_depth + depth;

    let full_radius = full_depth * tan;

    full_radius * 2.0
}

impl ProgramVisualizer {
    pub fn create(
        program_summary: &ProgramSummaryProto,
        lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
    ) -> Result<(Self, oneshot::Receiver<ProgramVisualization>)> {
        let (sender, receiver) = oneshot::channel();

        let raw_width =
            (program_summary.max_position().x() - program_summary.min_position().x()).max(0.0);
        let raw_height =
            (program_summary.max_position().y() - program_summary.min_position().y()).max(0.0);

        let width = (PIXELS_PER_MM * (2.0 * MARGIN_MM + raw_width)) as usize;
        let height = (PIXELS_PER_MM * (2.0 * MARGIN_MM + raw_height)) as usize;

        let mut canvas = graphics::raster::canvas::RasterCanvas::create(height, width);

        {
            let mut path = PathBuilder::new();
            path.rect(-1.0, -1.0, (width as f32) + 2.0, (height as f32) + 2.0);

            let mut p = canvas.create_path_fill(&path.build())?;
            p.draw(&Paint::color(Color::hex(0xffffff)), &mut canvas)?;
        }

        // mm to pixel unit conversion.
        canvas.translate(
            PIXELS_PER_MM * MARGIN_MM,
            (height as f32) - (PIXELS_PER_MM * MARGIN_MM),
        );
        canvas.scale(PIXELS_PER_MM, -1.0 * PIXELS_PER_MM);

        let inst = Self {
            summary: program_summary.clone(),
            lines,
            parser: gcode::ProgramParser::default(),
            canvas,
            output: sender,
        };

        Ok((inst, receiver))
    }

    pub async fn run(mut self) -> Result<()> {
        let mut last_position = Vector3f::default();

        loop {
            // TODO: Need to parse the final line with end_of_input

            let elements = match self.lines.recv().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(_) => return Ok(()),
            };

            for element in elements {
                let command = match element {
                    gcode::ProgramElement::Command(c) => c,
                    _ => continue,
                };

                // TODO: Need to track the current tool.

                // TODO: NEed to track when the spindle is on.

                /*
                0.2mm 30 degree

                */

                match &command {
                    gcode::Command::LinearMove(gcode::LinearMove { inner })
                    | gcode::Command::RapidMove(gcode::RapidMove { inner }) => {
                        let mut new_position = last_position.clone();
                        for (i, value) in [inner.x, inner.y, inner.z].into_iter().enumerate() {
                            if let Some(v) = value {
                                new_position[i] = v.to_f32();
                            }
                        }

                        let is_below = new_position.z() < 0.0 || last_position.z() < 0.0;

                        if is_below {
                            let diameter = calculate_engraving_diameter(
                                30.0,
                                0.2,
                                -1.0 * new_position.z().min(last_position.z()),
                            );

                            let mut path = PathBuilder::new();
                            path.move_to(last_position.block(0, 0).to_owned());
                            path.line_to(new_position.block(0, 0).to_owned());

                            let r = Vector2f::from_slice(&[diameter / 2.0, diameter / 2.0]);

                            let mut p = self.canvas.create_path_stroke(&path.build(), diameter)?;
                            p.draw(&Paint::color(Color::zero()), &mut self.canvas)?;

                            {
                                let mut path = PathBuilder::new();

                                path.ellipse(
                                    last_position.block(0, 0).to_owned(),
                                    r.clone(),
                                    0.0,
                                    2.0 * PI,
                                );
                                path.ellipse(
                                    new_position.block(0, 0).to_owned(),
                                    r.clone(),
                                    0.0,
                                    2.0 * PI,
                                );

                                let mut p = self.canvas.create_path_fill(&path.build())?;
                                p.draw(&Paint::color(Color::zero()), &mut self.canvas)?;
                            }
                        }

                        last_position = new_position;
                    }
                    _ => {}
                }
            }
        }

        let encoder = JPEGEncoder::new(90);

        let mut encoded = vec![];
        encoder.encode(&self.canvas.drawing_buffer, &mut encoded)?;

        let _ = self.output.send(ProgramVisualization {
            image: encoded,
            packed: encode_binary_image(&self.canvas.drawing_buffer),
        });

        Ok(())
    }
}
