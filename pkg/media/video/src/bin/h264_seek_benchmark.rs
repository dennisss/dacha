use std::time::Instant;

use common::errors::*;
use video::h264::H264BitStreamIterator;

fn main() -> Result<()> {
    let mut data = vec![];
    data.extend_from_slice(&[0, 0, 1]);
    for i in 0..100_000 {
        data.push(0xff);
    }

    data.extend_from_slice(&[0, 0, 0, 1]);
    data.push(0xff);

    let mut iter = H264BitStreamIterator::new(&data);

    let start = Instant::now();

    for i in 0..1000 {
        let p = iter.peek().unwrap();
        assert_eq!(p.data().len(), 100_000);
    }

    let end = Instant::now();

    // 114ms baseline.
    println!("{:?}", end - start);

    // let iter =

    Ok(())
}
