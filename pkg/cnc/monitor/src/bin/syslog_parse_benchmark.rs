#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::time::{Duration, Instant};

use base_error::*;
use protobuf::Message;

#[executor_main]
async fn main() -> Result<()> {
    /*


    */

    // let node = automata::regexp::node::RegExpNode::parse(
    //     "^((t|T|(?:[Tt]rue)|TRUE)|(f|F|(?:[Ff]alse)|FALSE)|(?:(-?[0-9.]+)(i)?))(?
    // :[ ,]|$)", )?;

    // let c = automata::regexp::vm::compiler::Compiler::compile(
    //     &node,
    //     automata::regexp::vm::flags::Flags::empty(),
    // )?;
    // println!("{}", c.assembly());

    // return Ok(());

    let data = b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=276396,tm=44701604817,v=4 pos_y v=290.312500 -260\npos_z v=17.433750 -249\npos_x v=94.587502 10790\npos_y v=290.312500 10802\npos_z v=17.433750 10806\npos_x v=94.587502 22686\npos_y v=290.312500 22692\npos_z v=17.433750 22705\npos_x v=94.587502 33718\npos_y v=290.312500 33729\npos_z v=17.433750 33733\npos_x v=94.587502 45689\npos_y v=290.312500 45701\npos_z v=17.433750 45705\npos_x v=94.587502 56734\npos_y v=290.312500 56740\npos_z v=17.433750 56751\npos_x v=94.587502 68750\npos_y v=290.312500 68757\npos_z v=17.433750 68761\nactive_extruder v=0i 79727\npos_x v=94.587502 80674\npos_y v=290.312500 80680\npos_z v=17.433750 80685\npos_x v=94.587502 91740\npos_y v=290.312500 91754\npos_z v=17.433750 91758\npos_x v=94.587502 103689\npos_y v=290.312500 103701\npos_z v=17.433750 103705\npos_x v=94.587502 114723\npos_y v=290.312500 114730\npos_z v=17.433750 114734\npos_x v=94.587502 126703\npos_y v=290.312500 126709\npos_z v=17.433750 126714\npos_x v=94.587502 138750\n";

    let profile = executor::spawn(perf::profile_self(Duration::from_secs(5)));

    let start = Instant::now();

    let mut n = 0;
    for i in 0..10000 {
        n += cnc_monitor::syslog_parser::parse_prusa_metrics_packet(data)?.len();
    }

    let end = Instant::now();

    assert!(n > 0);

    println!("{:?}", (end - start));

    let profile = profile.join().await?;
    file::write(project_path!("perf.pb"), profile.serialize()?).await?;

    Ok(())
}
