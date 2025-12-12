#[macro_use]
extern crate macros;

use std::time::{Duration, Instant};
use std::collections::HashMap;

use common::errors::*;
use file::{LocalPath, LocalPathBuf, LocalFile};
use peripherals_proto::peripherals::*;
use cnc_controller::bed::commands::*;
use common::io::Writeable;
use math_compute::io::CSVReader;
use common::hash::FastHasherBuilder;
use cnc_controller::toolhead::commands::*;
use electronics::*;
use cnc_controller::service::*;


/*

TODO: Validate that my 2Hz PWM wave is working as expected on the hardware.

TODO: Need following safety checks:
- Min/max temperature checks
- Allow retrying failed bed client calls after a short timeout
- Allow temporary loss of valid temperature information but log number of failures per time window


Bed temp resistor is 0.9993k
sheet resistor is 0.998
aux resistor is 0.997

Both are ±100ppm/°C so variance will be around 0.35% over the 25-60degree temp range we care about


Ambient bed temperature was '195',

5.168 in VDD \ 2.729 out

Actually measuring 4336 / 8191

zero point is around 7

actually measuring 2.64680747

TODO: Get 1.2K resistors since they are the most precise for PT1000 thermistors


----

Note that the relays can only switch at zero intersections.

- My power is 120V / 60Hz => 120 flips
- Configure a 2Hz PWM so we have 60 degrees of freedom

- At 2hz, period is 60 flips.
    - Support n/60



-----


cargo run --bin cnc_controller -- --mode=measure-bed --log_path=bed_in_frame.csv

cargo run --bin media_thermal --release -- record --output_path=bed_in_frame_thermal.log


cargo run --bin cnc_controller --release -- train-bed-model \
    --log_path=data/bed/bed_in_frame.csv \
    --step_output_dir=data/bed/in_frame/training_steps \
    --weights_output_path=data/bed/in_frame/training_weights.csv

cargo run --bin cnc_controller --release -- control-bed \
    --target_temperature=100 \
    --step_output_dir=data/bed/in_frame/control_to_100_steps \
    --results_path=data/bed/in_frame/control_to_100_data.csv

cargo run --bin media_thermal --release -- record --output_path=data/bed/in_frame/control_to_100_thermal.log


-----

cargo run --bin cnc_controller --release -- control-bed \
    --target_temperature=100 \
    --results_path=data/bed/in_frame/control_to_100_data_take2.csv

cargo run --bin media_thermal --release -- record --output_path=data/bed/in_frame/control_to_100_thermal_take2.log

cargo run --bin cnc_controller --release -- control-bed \
    --target_temperature=100 \
    --results_path=data/bed/in_frame/control_to_100_data_take3.csv

--------

cargo run --bin media_thermal --release -- record --output_path=data/bed/in_frame/control_to_100_thermal_take4.log

cargo run --bin cnc_controller --release -- control-bed \
    --target_temperature=100 \
    --results_path=data/bed/in_frame/control_to_100_data_take4.csv


cargo run --bin media_thermal --release -- \
    encode-mp4 \
    --input_path=data/bed/in_frame/control_to_100_thermal_take4.log \
    --output_path=data/bed/in_frame/control_to_100_thermal_take4.mp4

----

cargo run --bin media_thermal --release -- \
    encode-mp4 \
    --input_path=data/bed/bed_in_frame_thermal_compressed.log \
    --output_path=data/bed/bed_in_frame_thermal.mp4

data/bed/bed_in_frame_thermal_compressed.log

car


*/

/*
All the animations I need to create:

- Measurement (collecting training data):
    - Single graph with more points being added over time.
    - Just need 1 CSV
- Training Bed Model
    - 1 CSV per epoch
    - 1 CSV that contains per-epoch weights.

- Controlling Training
    - 1 CSV per epoch
    - Frame per epoch
    - Don't show the weights separately.

- Control real bed (combine heat up and down)
    - Just 1 CSV
    - Frame per time step
    - Inputs will be quantized.
    - TODO: Also print out an ETA

TODO: Change tool head to use a high temp magnet?

Encoder

3.3V, SCLK, GND, MISO, GND


- SCLK
- MISO
- MOSI
- CS (can get rid of it if I dedicate SPI pins)
- 
- 

Basically use some ribbon cable.


*/


#[derive(Args)]
struct Args {
    mode: Mode,
}

#[derive(Args)]
enum Mode {

    #[arg(name = "service")]
    Service(ControllerServiceCommand),

    #[arg(name = "measure-bed")]
    MeasureBed(MeasureBedCommand),

    #[arg(name = "train-bed-model")]
    TrainBedModel(TrainBedModelCommand),

    #[arg(name = "control-bed")]
    ControlBed(ControlBedCommand),

    #[arg(name = "bed-light-show")]
    BedLightShow(BedLightShowCommand),

    #[arg(name = "bed-fan-test")]
    BedFanTest(BedFanTestCommand),

    #[arg(name = "bed-heater-test")]
    BedHeaterTest(BedFanTestCommand),

    #[arg(name = "measure-toolhead")]
    MeasureToolhead(MeasureToolheadCommand),

    #[arg(name = "train-toolhead-heater-curve")]
    TrainToolheadHeaterCurve(TrainToolheadHeaterCurveCommand),

    #[arg(name = "train-toolhead-model")]
    TrainToolheadModel(TrainToolheadModelCommand),

    #[arg(name = "toolhead-pid")]
    ToolheadPID(ToolheadPIDCommand),

    #[arg(name = "control-toolhead-heater")]
    ControlToolheadHeater(ControlToolheadHeaterCommand),

    #[arg(name = "toolhead-test")]
    ToolheadTest,


}



use cnc_controller::tmc2209;
use cnc_controller::tmc2209::Register;
use common::array_ref;

use peripherals_service::device::PeripheralsDevice;



/*



Doing the PZ probe

- "Increase the probing speed - this may be particularly important for less rigid motion systems. Optimum probing speeds are between 3-7mm/s, it can be pushed up to 20mm/s (only if the motor current is set as low as possible to avoid damaging the bed).
"
- Full wave will be like 1ms
- Probably 0.2 - 0.4V spike
- Setup 8-bit ADC
- 40us (largest) acqusition time
- I need
    - Timer + PPI chanel to to setup sampling of the inputs at 1khz
    - 512 * 2 = 1024 bytes for an in-memory ADC buffer so bascially 

TODO: Dealing with negative votlages


How many GPIOTE do I need:
- 3 motors
    - 1 for STEP and 1 for DIAG
    - 
- so 6 
- Either way limit is 4 unless I do more UART multiplexing



TODO: Normally I don't need to use GPIOTE due to having the 'EVENTS_PORT' thing which can monitor basically all pins.

TODO: Do I need to set ENN=high to reset the stallguard error

*/


/*
TODO: We can use the magnetic angle sensor to detect if someone has inserted filament?s

Testing strategy:

- Temperature sensing (of the MCU)

- Still need to setup the analog support

- Then do tests on real tool head
    - Magnetic filament sensor
    - LEDs
    -Fans



TODO: I need to periodically check the health state of the TMC2209 over UART for overhrating etc.

- DRV_STATUS (ot / otpw) (or better to check the temp ones)


TODO: Need a safety feature to turn off the heater if we don't get any messages from the host to say to continue;

*/


use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorStatus};


async fn step(n: u32, device: &PeripheralsDevice, driver: &TMC2209Device) -> Result<()> {

    let time = device.get_clock_time().await?.remote_time;

    let mut start_time = time + 4_000_000;

    {
        let mut m = StepperMotorMotion::default();
        m.set_next_step_time(start_time);
        m.set_num_steps_minus_one(n - 1);
        m.set_next_step_duration(4_000u32);
        driver.enqueue_stepper_motion(m).await?;
    }

    loop {
        let status = driver.get_stepper_motor_status().await?;
        if !status.active() {
            break;
        }

        executor::sleep(Duration::from_millis(100)).await;
    }

    Ok(())

}


use std::sync::Arc;

use cnc_controller::tmc2209::TMC2209Device;
use peripherals_proto::peripherals::TMC2209Config;


use cnc_controller::ma732::MA732;


/*

I_RUN = 16
I_HOLD = 8

i_rms = ((cs + 1) / 32) * (0.325 / (0.11 + 0.02)) * (1 / sqrt(2))
i_rms = ((16 + 1) / 32) * (0.325 / (0.11 + 0.02)) * (1 / math.sqrt(2))


TODO: Constantly high DIAG implies there is something messed up with the motor. 
*/
async fn toolhead_test() -> Result<()> {
    let mut configs = peripherals_service::config::BoardConfigRegistry::defaults().await?;

    let config = configs.remove("voron0_toolhead")
        .ok_or_else(|| err_msg("No config with the given name"))?;


        /*
    let mut stepper_config = TMC2209Config::default();
    protobuf::text::parse_text_proto(r#"
        step_peripheral: "stepper_step"
        uart_peripheral: "stepper_uart"
        diag_peripheral: "stepper_diag"
        enable_peripheral: "stepper_enable"
    "#, &mut stepper_config)?;
    */

    let (mut device, _) = PeripheralsDevice::create(&config).await?;

    let device = Arc::new(device);

    // let stepper = TMC2209Device::create(stepper_config, device.clone()).await?;
    // let mag = MA732::new(device.clone());

    /*
    SK6812MINI-E : 24-bit
    - GRB

    IN-PI33QBTPRPGPBPW : 32-bit
    - GRBW


    MA732
    - 25Mhz SPI max clock speed

    - 14-bit max 

    - Recommended magnet
        - 5 diameter x 3mm cyliner (N35)
        - with 1.5mm air gap to the sensor
        - off camera glued down with silicon glue

    - Package height is 1mm
    - PCB surface is 3.8mm above motor
    - Motor cavity is 0.6mm

    - So total available depth of 3.4mm

    Use a 1/16" thick by 1/4" diameter magnet

    The motor cavity outer diameter is 7mm so bascially need to eyeball that there is a small gap on all sides

    All SPI frames are 16-bits (reading and writing at same time)

    */



    /*
    loop {
        println!("..");

        stepper.device.neopixel_transfer("leds", &[
            0xff, 0x00, 0x00,
            0x00, 0xff, 0x00, 0x00,
            0x00, 0x00, 0xff, 0x00,
        ]).await?;

        executor::sleep(Duration::from_millis(1000)).await;

        // stepper.device.neopixel_transfer("leds", &[
        //     0x00, 0xff, 0x00, /* 0xff,
        //     0xff, 0xff, 0xff, 0xff,
        //     0xff, 0xff, 0xff, 0xff, */
        // ]).await?;

        // executor::sleep(Duration::from_millis(1000)).await;

        //         stepper.device.neopixel_transfer("leds", &[
        //     0x00, 0x00, 0xff, /* 0xff,
        //     0xff, 0xff, 0xff, 0xff,
        //     0xff, 0xff, 0xff, 0xff, */
        // ]).await?;

        // executor::sleep(Duration::from_millis(1000)).await;

    }
    */



    /*
    loop {
        // println!("{:?}", device.get_clock_time().await?);
        // println!("{:?}", device.get_idle_counter().await?);

        // executor::sleep(Duration::from_millis(500)).await?;

        let s = Instant::now();
        let triggered = device.analog_read_window("probe").await?;
        let e = Instant::now();
        println!(".. : {:?} {:?}", triggered, e - s);

        // if triggered {
        //     println!("{:?}", device.analog_fetch_window("probe").await?);
        // }
    }
    */

    /*


    loop {
        println!("{:?}", stepper.read_analog().await?);
        executor::sleep(Duration::from_millis(1000)).await;
    }

    return Ok(());
    */


    /*

    stepper.enable().await?;


    for i in 0..20 {

        step(3200 / 8, &device, &stepper).await?;

        executor::sleep(Duration::from_millis(500)).await;

        // let angle = mag.get_angle().await?;
        // println!("{:.2?}", angle * 360.0);

        // executor::sleep(Duration::from_millis(500)).await;
    }

    stepper.disable().await?;

    */

    /*
    DRV_STATUS  0x6F
    IFCNT 0x02 (1 byte)
    */

    Ok(())
}


#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.mode {
        Mode::Service(cmd) => cmd.run().await,
        Mode::MeasureBed(cmd) => {
            cmd.run().await
        }
        Mode::TrainBedModel(cmd) => {
            cmd.run().await
        }
        Mode::ControlBed(cmd) => {
            cmd.run().await
        }
        Mode::BedLightShow(cmd) => {
            cmd.run().await
        }
        Mode::BedFanTest(cmd) => {
            cmd.run().await
        }
        Mode::BedHeaterTest(cmd) => {
            cmd.run().await
        }
        Mode::MeasureToolhead(cmd) => {
            cmd.run().await
        }
        Mode::TrainToolheadHeaterCurve(cmd) => {
            cmd.run().await
        }
        Mode::TrainToolheadModel(cmd) => {
            cmd.run().await
        }
        Mode::ToolheadPID(cmd) => cmd.run().await,
        Mode::ControlToolheadHeater(cmd) => cmd.run().await,
        Mode::ToolheadTest => {
            return toolhead_test().await;
        }
    }
}