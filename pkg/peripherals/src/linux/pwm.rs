use std::time::Duration;

use common::errors::*;
use file::{LocalPath, LocalPathBuf};

const NANOS_PER_SECOND: usize = 1000000000;

pub struct PWMChannel {
    channel_dir: LocalPathBuf,
}

impl PWMChannel {
    /// NOTE: This assumes that any GPIO pins that need to be wired up to the channel are
    /// configured separately.
    pub async fn open(chip: usize, channel: usize) -> Result<Self> {
        if !file::exists("/sys/class/pwm").await? {
            return Err(err_msg("PWM sys fs driver not detected."));
        }

        let chip_dir = format!("/sys/class/pwm/pwmchip{}", chip);
        if !file::exists(&chip_dir).await? {
            return Err(err_msg("PWM chip does not exist"));
        }

        // Export the channel.
        // When already exported this will fail with an Os::ResourceBusy error. Instead
        // of checking the error code, we just verify later that the channel
        // sub-directory exists.
        let export_path = LocalPath::new(&chip_dir).join("export");
        file::write(&export_path, format!("{}\n", channel))
            .await
            .ok();

        // Wait for exporting to compelte.
        executor::sleep(Duration::from_millis(100)).await;

        let channel_dir = LocalPath::new(&chip_dir).join(format!("pwm{}", channel));
        if !file::exists(&channel_dir).await? {
            return Err(format_err!(
                "Failed to export PWM channel {}",
                channel
            ));
        }

        Ok(Self { channel_dir })
    }

    /// Configures the current PWM value
    ///
    /// frequency: Frequency of the square wave in Hz
    /// duty_cycle: Percentage of the time the square wave should be up (from
    /// 0.0 to 1.0).
    pub async fn write(&mut self, frequency: f32, duty_cycle: f32) -> Result<()> {
        // Convert to nanoseconds.
        let period = ((NANOS_PER_SECOND as f32) / frequency) as usize;
        let duty_cycle = ((period as f32) * duty_cycle) as usize;

        file::write(self.channel_dir.join("period"), format!("{}\n", period)).await?;
        file::write(
            self.channel_dir.join("duty_cycle"),
            format!("{}\n", duty_cycle),
        )
        .await?;
        file::write(self.channel_dir.join("enable"), "1\n").await?;
        Ok(())
    }
}