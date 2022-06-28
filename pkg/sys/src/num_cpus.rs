
use common::errors::*;

pub fn num_cpus() -> Result<usize> {
    // TODO: Verify that all CPUs are numbered from 0 to N-1
    // TODO: Try /proc/cpuinfo if this is not evaluate.

    let mut total = 0;

    let data = std::fs::read_to_string("/proc/stat")?;
    for line in data.lines() {
        if let Some(rest) = line.strip_prefix("cpu") {
            if let Some(c) = rest.chars().next() {
                if c.is_numeric() {
                    total += 1;
                }
            }
        }
    }

    Ok(total)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_num_cpus() {
        println!("Num CPUs: {}", num_cpus().unwrap());
    }
}