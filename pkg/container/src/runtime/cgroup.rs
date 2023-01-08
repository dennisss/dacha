use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use common::errors::*;
use file::{LocalPath, LocalPathBuf};

pub struct Cgroup {
    dir: LocalPathBuf,
    value: Option<CgroupMeasurement>,
    previous_value: Option<CgroupMeasurement>,
}

impl Cgroup {
    pub fn new(dir: LocalPathBuf) -> Self {
        Self {
            dir,
            value: None,
            previous_value: None,
        }
    }

    pub async fn set_max_pids(&mut self, num: usize) -> Result<()> {
        file::write(self.dir.join("pids.max"), num.to_string()).await
    }

    /// Collects another measurement and adds it to this collection.
    pub async fn collect_measurement(&mut self) -> Result<()> {
        self.previous_value = self.value.take();
        self.value = Some(CgroupMeasurement::read(&self.dir).await?);
        Ok(())
    }

    pub fn cpu_usage(&self) -> f32 {
        let value = match self.value.as_ref() {
            Some(v) => v,
            None => return 0.0,
        };

        let previous_value = match self.previous_value.as_ref() {
            Some(v) => v,
            None => return 0.0,
        };

        let t = (value.cpu_system_usage + value.cpu_user_usage)
            - (previous_value.cpu_system_usage + previous_value.cpu_user_usage);

        let t2 = value.time - previous_value.time;

        t.as_secs_f32() / t2.as_secs_f32()
    }

    pub fn memory_usage(&self) -> u64 {
        let value = match self.value.as_ref() {
            Some(v) => v,
            None => return 0,
        };

        value.memory_usage
    }
}

struct CgroupMeasurement {
    /// Time when these metrics were recorded.
    time: Instant,

    cpu_user_usage: Duration,

    cpu_system_usage: Duration,

    memory_usage: u64,
}

impl CgroupMeasurement {
    async fn read(cgroup_dir: &LocalPath) -> Result<Self> {
        // TODO: We could use 'openat' to make this faster.

        let time = Instant::now();

        let cpu_stats = file::read_to_string(cgroup_dir.join("cpu.stat")).await?;
        let cpu_stats_map = parse_key_value_file(&cpu_stats)?;

        let cpu_user_usage =
            Duration::from_micros(cpu_stats_map.get_or_err("usage_usec")?.parse()?);
        let cpu_system_usage =
            Duration::from_micros(cpu_stats_map.get_or_err("system_usec")?.parse()?);

        let memory_usage = file::read_to_string(cgroup_dir.join("memory.current"))
            .await?
            .trim_end()
            .parse()?;

        // If we take too long to read the metrics, then they may be stale and not
        // representative of the state at the recorded timestamp.
        let end_time = Instant::now();
        if end_time - time > Duration::from_millis(10) {
            return Err(format_err!(
                "Took too long ({:?}) to read cgroup metrics.",
                end_time - time
            ));
        }

        Ok(Self {
            time,
            cpu_system_usage,
            cpu_user_usage,
            memory_usage,
        })
    }
}

fn parse_key_value_file(data: &str) -> Result<HashMap<&str, &str>> {
    let mut map = HashMap::default();

    for line in data.lines() {
        if line.is_empty() {
            continue;
        }

        let (key, value) = line
            .split_once(' ')
            .ok_or_else(|| format_err!("Line does not contain a space: {}", line))?;

        if map.contains_key(&key) {
            return Err(format_err!("Duplicate key: {}", key));
        }

        map.insert(key, value);
    }

    Ok(map)
}
