use std::time::Duration;

use common::errors::*;
use file::LocalPath;

pub async fn get_partition_fstype(partition_path: &LocalPath) -> Result<String> {
    let mut t = String::new();

    for _ in 0..20 {

        /*
        let output = std::process::Command::new("lsblk")
            .args(["-no", "FSTYPE", partition_path.to_str().unwrap()])
            .output()?;
        */

        let output = command_args!(
            "blkid -s TYPE -o value {partition_path.to_str().unwrap()}"
        ).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("attempt failed: {}", stderr.trim());
            // Takes some time for the kernel and udev to sync everything.
            executor::sleep(Duration::from_secs(1)).await?;
            continue;
        }

        t = std::str::from_utf8(&output.stdout)?.trim().to_string();

        if t.is_empty() {
            println!("returned an empty fs");
            executor::sleep(Duration::from_secs(1)).await?;
            continue;
        }
        break;
    }

    Ok(t)
}