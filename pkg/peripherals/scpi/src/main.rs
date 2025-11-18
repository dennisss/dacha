#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::{Instant, Duration};
use std::process::{Command, Stdio, Child, ChildStdin};
use std::io::Write;

use common::errors::*;

use scpi::*;


/*
cargo run --bin scpi -- record-screen --addr=10.1.0.135 --output_path=scope.mp4


cargo run --bin scpi --release -- record-screen --addr=10.1.0.135 --output_path=hall_effect_demo.mp4
*/

#[derive(Args)]
struct Args {
    command: ArgCommand
}

#[derive(Args)]
enum ArgCommand {
    #[arg(name = "record-screen")]
    RecordScreen(RecordScreenCommand),

    #[arg(name = "measure-temp")]
    MeasureTemp(MeasureTempCommand),
}

#[derive(Args)]
struct RecordScreenCommand {
    addr: String,
    output_path: String,
}

impl RecordScreenCommand {

    pub async fn run(self) -> Result<()> {
        let mut client = SCPIClient::create(&self.addr).await?;
        println!("Device: {:?}", client.identity().await?);

        let mut ffmpeg = FFMpegInstance::create(&self.output_path)?;

        let mut frame_time = Instant::now();
        let frame_duration = Duration::from_secs_f32(1.0 / 4.0);

        let cancellation_token = executor::signals::new_shutdown_token();

        let mut i = 0;
        while !cancellation_token.is_cancelled().await {
            // if i % 10 == 0 {
            //     println!("Frame idx: {}", i);
            // }

            let data = client.run_binary_command("SCDP", Some(768067)).await?;
            
            // Drop the last byte which is a '\n' terminator
            let bmp_data = &data[0..768066];
            
            ffmpeg.stdin.write_all(bmp_data)?;


            i += 1;
            frame_time += frame_duration;

            let now = Instant::now();
            if now > frame_time {
                println!("!!!! Slow frame: {:?}", now - frame_time);
            } else {
                executor::sleep(frame_time - now).await?;
            }
        }

        println!("Wrapping up...");

        ffmpeg.finish()?;

        Ok(())
    }
}

#[derive(Args)]
struct MeasureTempCommand {
    addr: String
}

impl MeasureTempCommand {


    pub async fn run(self) -> Result<()> {
        let mut client = SCPIClient::create(&self.addr).await?;

        for cmd in [
            "CONF:TEMP THER,KITS90",
            "TRIG:SOUR IMM",
            "TRIG:COUN INF",
            "INIT"
        ] {
            client.run_command_noreply(cmd).await?;
        }

        println!("{:?}", client.identity().await?);

        let raw = client.run_command("DATA:LAST?").await?;
        
        let v = raw.strip_suffix(" C")
            .ok_or_else(|| err_msg("Invalid temp measurement format"))?
            .trim()
            .parse::<f32>()?;

        println!("{}", v);

        Ok(())
    }

}

/*
cargo run --bin scpi -- measure-temp --addr=10.1.0.134
*/

// 10.1.0.134

struct FFMpegInstance {
    process: Child,
    stdin: ChildStdin
}

impl FFMpegInstance {

    pub fn create(output_path: &str) -> Result<Self> {

        let mut process = Command::new("ffmpeg")
            .args([
                "-f", "image2pipe",
                "-framerate", "5",
                "-i", "-",
                "-c:v", "libx264",
                "-crf", "18",
                "-pix_fmt", "yuv420p",
                output_path
            ])
            .stdin(Stdio::piped())
            .spawn()?;

        let mut stdin = process.stdin.take().unwrap();

        Ok(Self {
            process,
            stdin
        })
    }

    pub fn finish(mut self) -> Result<()> {
        drop(self.stdin);
        let status = self.process.wait()?;
        if !status.success() {
            return Err(format_err!("ffmpeg failed: {:?}", status));
        }

        Ok(())
    }

}




#[executor_main]
async fn main() -> Result<()> {

    let args = common::args::parse_args::<Args>()?;
    match args.command {
        ArgCommand::RecordScreen(cmd) => cmd.run().await,
        ArgCommand::MeasureTemp(cmd) => cmd.run().await,
    }

    // FETC?
}


