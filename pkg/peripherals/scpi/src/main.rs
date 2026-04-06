#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::{Instant, Duration, SystemTime, UNIX_EPOCH};
use std::process::{Command, Stdio, Child, ChildStdin};
use std::io::Write;

use common::errors::*;
use base_args::define_arg_command;
use file::{LocalPath, LocalPathBuf};
use scpi::*;


/*
cargo run --bin scpi -- record-screen --addr=10.1.0.135 --output_path=scope.mp4


cargo run --bin scpi --release -- record-screen --addr=10.1.0.135 --output_path=hall_effect_demo.mp4


cargo run --bin scpi -- measure-voltage --addr=10.1.0.135

cargo run --bin scpi -- measure-voltage --addr=10.1.0.135 --interval_secs=5 --output_path=lipo_voltage.csv


*/

#[derive(Args)]
struct Args {
    command: ArgCommand
}

define_arg_command!(ArgCommand {
    RecordScreenCommand = "record-screen",
    MeasureTempCommand = "measure-temp",
    MeasureVoltageCommand = "measure-voltage",
});

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

#[derive(Args)]
struct MeasureVoltageCommand {
    addr: String,
    interval_secs: Option<f64>,
    output_path: Option<LocalPathBuf>,
}

impl MeasureVoltageCommand {


    pub async fn run(self) -> Result<()> {

        if let Some(interval_secs) = self.interval_secs.clone() {
            let output_path = self.output_path.as_ref().ok_or_else(|| err_msg("No output path specified"))?;

            file::write(&output_path, "time,voltage\n").await?;

            loop {
                eprintln!("Sampling loop failed: {:?}",
                    self.run_sampling(Duration::from_secs_f64(interval_secs),  output_path.as_ref()).await);
                executor::sleep(Duration::from_secs(5)).await?;
            }

            return Ok(());
        }

        let mut client = SCPIClient::create(&self.addr).await?;

        let v = client.measure_voltage().await?;
        println!("{}", v);

        Ok(())
    }

    async fn run_sampling(&self, interval: Duration, output_path: &LocalPath) -> Result<()> {
        let mut client = SCPIClient::create(&self.addr).await?;

        loop {
            let v = client.measure_voltage().await?;
            file::append(output_path, format!("{},{}\n", Self::now(), v).as_bytes()).await?;
            println!("V: {}", v);
            executor::sleep(interval).await?;
        }
    }

    fn now() -> f64 {
        let now = SystemTime::now();
        now.duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
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
    args.command.run().await

    // FETC?
}


