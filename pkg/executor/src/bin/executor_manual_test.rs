extern crate common;
extern crate executor;
extern crate sys;

use std::time::Duration;

use common::errors::*;

/*
We want to ensure that even if we don't have panic = "abort", if we detect thread failures, we should stop the main thread as well.
*/
async fn run() -> Result<()> {
    let task = executor::spawn(async move {
        executor::sleep(Duration::from_millis(1)).await;

        // println!("{}", unsafe { *core::ptr::null::<u64>() });

        panic!("I failed!");
    });

    executor::sleep(Duration::from_millis(100000)).await;

    Ok(())
}

fn main() -> Result<()> {
    executor::run_main(run())?
}
