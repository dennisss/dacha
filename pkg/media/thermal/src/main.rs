#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::Duration;
use std::collections::VecDeque;
use std::process::Command;
use std::process::Stdio;
use std::io::Write;
use std::time::Instant;

use base_units::ByteCount;
use base_error::*;
use file::{LocalPath, LocalPathBuf};
use image::format::jpeg::encoder::JPEGEncoder;
use image::Color;
use media_proto::media::ImageFrameProto;
use media_thermal::*;


#[derive(Args)]
struct Args {
    mode: Mode
}

#[derive(Args)]
enum Mode {
    #[arg(name = "record")]
    Record(RecordCommand),

    #[arg(name = "compress-video")]
    CompressVideo(CompressVideoCommand),

    #[arg(name = "encode-mp4")]
    EncodeMP4(EncodeMP4Command)
}

#[derive(Args)]
struct RecordCommand {
    /// NOTE: This is only used in the viewer UI.
    min_temp: f32,
    max_temp: f32,
    output_path: LocalPathBuf
}


#[derive(Args)]
struct CompressVideoCommand {
    input_path: LocalPathBuf,
    output_path: LocalPathBuf
}

#[derive(Args)]
struct EncodeMP4Command {
    min_temp: f32,
    max_temp: f32,

    input_path: LocalPathBuf,
    output_path: LocalPathBuf
}

async fn open_camera() -> Result<v4l2::UnconfiguredStream> {
    let ctx = usb::Context::create()?;

    let mut selected_device = None;

    let devices = ctx.enumerate_devices().await?;
    for dev in devices {
        let desc = dev.device_descriptor()?;

        // Thermal Master P2
        if desc.idVendor == 0x3474 && desc.idProduct == 0x4281 {
            selected_device = Some(dev);
            break;
        }
    }

    let device = selected_device.ok_or_else(|| err_msg("No thermal camera found"))?;


    let mut v4l2_paths = vec![];

    let drivers = device.driver_devices().await?;
    for driver in drivers {
        if driver.typ == usb::DriverDeviceType::V4L2 {
            v4l2_paths.push(driver.path);
        }
    }

    let mut selected_capture_stream = None;

    for path in v4l2_paths {
        let mut dev = v4l2::Device::open(&path).await?;

        if !dev.supports_capture_stream() {
            continue;
        }

        let mut capture_stream = dev.new_capture_stream()?;

        let formats = capture_stream.list_formats().await?;
        if formats.len() == 0 {
            continue;
        }

        // Assumption is that there is only one supported 'YUYV' pixel format.

        let mut format = capture_stream.get_format().await?;
        format.set_width(256);
        format.set_height(386);
        capture_stream.set_format(format.clone()).await?;

        // TODO: The camera can't go up to 30. It can only go up to around 25.1
        {
            let mut params = capture_stream.get_streaming_params().await?;
            let capture_param = unsafe { &mut params.parm.capture };

            if capture_param.capability & v4l2::V4L2_CAP_TIMEPERFRAME == 0 {
                return Err(err_msg("Device doesn't support setting the frame rate"));
            }

            capture_param.timeperframe.numerator = 1;
            capture_param.timeperframe.denominator = 30;

            capture_stream.set_streaming_params(params).await?;

        }

        selected_capture_stream = Some(capture_stream);
        break;
    }

    selected_capture_stream.ok_or_else(|| err_msg("No capture stream found"))
}

async fn record(cmd: RecordCommand) -> Result<()> {


    let viewer = Viewer::create()?;


    let capture_stream = open_camera().await?;

    let (mut capture_stream, capture_buffers) = capture_stream.configure_mmap(8).await?;
    for buf in capture_buffers {
        capture_stream.enqueue_buffer(buf).await?;
    }

    capture_stream.turn_on().await?;

    let mut video_writer = VideoWriter::create_new(&cmd.output_path, VideoWriterOptions {
        deflate: true,
    }).await?;

    // Last N frame times for estimating the FPS.
    let mut frame_times = VecDeque::new();

    let mut next_sequence = 0;
    let mut num_skipped = 0;

    let cancellation_token = executor::signals::new_shutdown_token();

    while !cancellation_token.is_cancelled().await {

        let buf = capture_stream.dequeue_buffer().await?;

        num_skipped += (buf.sequence() - next_sequence);
        next_sequence = buf.sequence() + 1;

        if buf.used_memory().len() != 386 * 256 * 2 {
            return Err(err_msg("Wrong buffer size"));
        }

        let frame_size = 192 * 256 * 2;

        let start_offset = frame_size + 2 * (2 * 256);
        let end_offset = start_offset + frame_size;

        let value_buffer = &buf.used_memory()[start_offset..end_offset];


        let mut min_temp = 1000.0f32;
        let mut max_temp = 0.0f32;

        let mut img = image::Image::<u8>::zero(
            192 as usize,
            256 as usize,
            image::Colorspace::RGB,
        );

        for (i, pixel) in value_buffer.chunks(2).enumerate() {
            let v = u16::from_le_bytes(*array_ref![pixel, 0, 2]);

            let t = ((v as f32) / 64.0) - 273.15;

            {
                let y = i / 256;
                let x = i % 256;

                // Scale to the 20 to 110C range.
                let tt = (t - cmd.min_temp) * (1.0 / (cmd.max_temp - cmd.min_temp));
                let c = inferno_color(tt);

                // let m = (t.max(0.0).min(255.0) * 2.0) as u8;
                img.set(y, x, &c);
            }

            min_temp = min_temp.min(t);
            max_temp = max_temp.max(t);
        }

        viewer.set_image(img);

        let timestamp = buf.monotonic_timestamp().ok_or_else(|| err_msg("Missing frame timestamp"))?;


        {
            let mut proto = ImageFrameProto::default();
            proto.set_timestamp(timestamp.as_micros() as u64);
            proto.set_data(value_buffer);
            video_writer.append(proto).await?;
        }

        /*
        if buf.sequence() % 100 == 0 {
            let encoder = JPEGEncoder::new(80);
            let mut data = vec![];
            encoder.encode(&img, &mut data)?;

            file::write("thermal.jpg", &data).await?;

        }
        */


        let fps = {
            let window_size = 200;
            while frame_times.len() >= window_size {
                frame_times.pop_front();
            }
            frame_times.push_back(timestamp);

            let mut fps = 0.0;

            if frame_times.len() >= window_size {
                let dt = (frame_times[frame_times.len() - 1] - frame_times[0]).as_secs_f32();
                fps = (window_size as f32) / dt;
            }

            fps
        };

        if buf.sequence() % 100 == 0 {
            println!(
                "[Seq: {}] [Min: {}] [Max: {}] [FPS: {:.2}] [Skipped: {}]",
                buf.sequence(), min_temp, max_temp, fps, num_skipped
            );
        }

        capture_stream.enqueue_buffer(buf).await?;
    }

    capture_stream.turn_off().await?;
    drop(capture_stream);

    println!("Flushing video...");
    video_writer.flush().await?;
    println!("Done!");


    Ok(())
}


async fn compress_video(cmd: CompressVideoCommand) -> Result<()> {

    let mut video_reader = VideoReader::open(&cmd.input_path).await?;

    let mut video_writer = VideoWriter::create_new(&cmd.output_path, VideoWriterOptions {
        deflate: true,
    }).await?;

    while let Some(frame) = video_reader.next().await? {
        video_writer.append(frame).await?;
    }

    video_writer.flush().await?;

    Ok(())
}

// TODO: Dedup this.
struct ProgressTracker {
    start_time: Instant,
    total_bytes: usize,

    last_time: Instant,
    last_percentage: usize,
    last_written_bytes: usize,
}

impl ProgressTracker {
    fn new(total_bytes: usize) -> Self {
        let t = Instant::now();
        Self {
            start_time: t.clone(),
            total_bytes,

            last_time: t.clone(),
            last_percentage: 0,
            last_written_bytes: 0,
        }
    }

    fn update(&mut self, written_bytes: usize) {
        let percent = (100 * written_bytes) / self.total_bytes;
        if percent == self.last_percentage {
            return;
        }

        let time = Instant::now();

        let rate = ((written_bytes - self.last_written_bytes) as f64)
            / (time - self.last_time).as_secs_f64();
        println!("=> {}% [{:?}/s]", percent, ByteCount::from(rate as usize));

        if percent == 100 {
            println!("Done! Took: {:?}", time - self.start_time);
        }

        self.last_percentage = percent;
        self.last_written_bytes = written_bytes;
        self.last_time = time;
    }
}

async fn encode_mp4(cmd: EncodeMP4Command) -> Result<()> {

    let mut video_reader = VideoReader::open(&cmd.input_path).await?;
    let video_file_size = file::metadata(&cmd.input_path).await?.len();

    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-f", "rawvideo", "-pix_fmt", "rgb24",
            "-s", "256x192",
            "-r", "25.1",
            "-i", "-", "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p"
        ])
        .arg(cmd.output_path.as_str())
        .stdin(Stdio::piped())
        .spawn()?;

    let mut stdin = ffmpeg.stdin.take().unwrap();

    let mut tracker = ProgressTracker::new(video_file_size as usize);

    while let Some(frame) = video_reader.next().await? {

        let value_buffer = frame.data();

        let mut img = image::Image::<u8>::zero(
            192 as usize,
            256 as usize,
            image::Colorspace::RGB,
        );

        for (i, pixel) in value_buffer.chunks(2).enumerate() {
            let v = u16::from_le_bytes(*array_ref![pixel, 0, 2]);

            let t = ((v as f32) / 64.0) - 273.15;

            {
                let y = i / 256;
                let x = i % 256;

                // Scale to the 20 to 110C range.
                let tt = (t - cmd.min_temp) * (1.0 / (cmd.max_temp - cmd.min_temp));
                let c = inferno_color(tt);

                // let m = (t.max(0.0).min(255.0) * 2.0) as u8;
                img.set(y, x, &c);
            }
        }

        stdin.write_all(&img.array.data)?;

        tracker.update(video_reader.offset() as usize);
    }

    drop(stdin);
    let status = ffmpeg.wait()?;
    if !status.success() {
        return Err(format_err!("ffmpeg failed: {:?}", status));
    }

    Ok(())

}


#[executor_main]
async fn main() -> Result<()> {

    let args = common::args::parse_args::<Args>()?;

    match args.mode {
        Mode::Record(cmd) => record(cmd).await,
        Mode::CompressVideo(cmd) => compress_video(cmd).await,
        Mode::EncodeMP4(cmd) => encode_mp4(cmd).await
    }
}
