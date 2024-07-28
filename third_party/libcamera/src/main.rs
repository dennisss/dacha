use base_error::*;

use libcamera::FromControlValue;

// \_SB_.PCI0.GP13.XHC0.RHUB.PRT3-3.2.1.4:1.0-32e4:9422

/*
For Pi Camera V1

0 : ov5647 [2592x1944 10-bit GBRG] (/base/soc/i2c0mux/i2c@1/ov5647@36)
    Modes: 'SGBRG10_CSI2P' : 640x480 [58.92 fps - (16, 0)/2560x1920 crop]
                             1296x972 [43.25 fps - (0, 0)/2592x1944 crop]
                             1920x1080 [30.62 fps - (348, 434)/1928x1080 crop]
                             2592x1944 [15.63 fps - (0, 0)/2592x1944 crop]
*/

// See
// https://github.com/raspberrypi/rpicam-apps/blob/a2b156fc7607ecd2de8d389767924ef9f66588cd/core/options.cpp#L460

fn print_camera_info(camera: libcamera::AvailableCamera) -> Result<()> {
    println!("Id: {}", camera.id());

    let mut camera = camera.acquire()?;
    println!("Acquired!");

    /*
    if let Some(model) = camera.properties().get(libcamera::properties::Model) {
        println!("Model: {}", model);
    }
    */

    if let Some(area) = camera
        .properties()
        .get(libcamera::properties::PixelArrayActiveAreas)
    {
        if area.len() == 1 {
            // TODO: Expose a nice toString function for this.
            println!("Sensor Area: {:?}", area[0]);
        }
    }

    if let Some(size) = camera
        .properties()
        .get(libcamera::properties::PixelArraySize)
    {
        println!("Sensor Area: {:?}", size);
    }

    println!("Properties: {:#?}", camera.properties());

    let config = camera
        .generate_configuration(&[libcamera::StreamRole::Raw])
        .ok_or_else(|| err_msg("Failed to generate a default camera config"))?;

    // Second copy of the config that we can mutate while iterating over 'config'
    let mut config2 = camera
        .generate_configuration(&[libcamera::StreamRole::Raw])
        .ok_or_else(|| err_msg("Failed to generate a default camera config"))?;

    if config.stream_configs_len() != 1 {
        return Err(err_msg(
            "Expected camera config to have one one stream config",
        ));
    }

    println!("Sensor Modes:");

    let formats = config.stream_config(0).formats();
    for pixel_format in formats.pixel_formats() {
        println!("  - Pixel Format: {:?}", pixel_format);

        for size in formats.sizes(pixel_format) {
            println!("    - Size: {:?}", size);

            config2.stream_config_mut(0).set_pixel_format(pixel_format);
            config2.stream_config_mut(0).set_size(size);

            let mut sensor_config = libcamera::SensorConfiguration::default();
            sensor_config.outputSize = size;
            sensor_config.bitDepth = libcamera::pixel_format_bit_depth(pixel_format) as u32;
            config2.set_sensor_config(Some(sensor_config));
            if config2.validate() != libcamera::CameraConfigurationStatus::Valid {
                println!("      => Invalid config");
                continue;
            }

            let c = camera.configure(&mut config2)?;

            for (control_id, control_info) in c.controls().iter() {
                // TODO: Also print the control_info.values() if non-empty.
                println!(
                    "      - {}; min: {:?}; max: {:?}; def: {:?}",
                    control_id.name(),
                    libcamera::ControlValueEnum::from_value(control_info.min()).unwrap(),
                    libcamera::ControlValueEnum::from_value(control_info.max()).unwrap(),
                    libcamera::ControlValueEnum::from_value(control_info.def()).unwrap()
                );
            }

            // println!("{:?}", c.controls());

            // TODO: Use the min of this which should be a number in microseconds to
            // estimate max FPS if let Some(frame_dur_info) =
            // c.controls().get(libcamera::controls::FrameDurationLimits) {
            //     frame_dur_info.min().
            // }

            // if c.controls().get

            // let sensor_config = libcamera::bin

            camera = c.unconfigure();
        }
    }

    {
        let config = camera
            .generate_configuration(&[libcamera::StreamRole::Viewfinder])
            .ok_or_else(|| err_msg("Failed to generate a default camera config"))?;

        if config.stream_configs_len() != 1 {
            return Err(err_msg(
                "Expected camera config to have one one stream config",
            ));
        }

        println!("");
        println!("Supported output formats:");

        let formats = config.stream_config(0).formats();
        for pixel_format in formats.pixel_formats() {
            println!("  - Pixel Format: {:?}", pixel_format);
        }
    }

    camera.release()?;

    Ok(())
}

fn main() -> Result<()> {
    libcamera::disable_logging();

    let manager = libcamera::CameraManager::create()?;

    let mut cameras = manager.cameras();

    for camera in cameras {
        print_camera_info(camera)?;

        println!("========================");
    }

    /*

    // TOOD: Ignore ones on Pi that contain "/usb"
    let camera = cameras.pop().unwrap();
    println!("Id: {}", camera.id());

    // println!("Static Num Streams: {}", camera.streams().len());
    // for stream in camera.streams() {
    //     println!("S: {:x}", stream.id())
    // }

    println!("Controls: {:#?}", camera.controls());

    let mut config = camera
        .generate_configuration(&[libcamera::StreamRole::Raw])
        .unwrap();
    assert_eq!(config.stream_configs_len(), 1);

    // Only allocate one buffer per stream.
    config.stream_config_mut(0).set_buffer_count(1);

    println!("Supported Formats:");
    for format in config.stream_config(0).formats().pixel_formats() {
        println!("- {:?}", format);

        if format.to_string() == "NV21" {
            config.stream_config_mut(0).set_pixel_format(format);
        }
    }

    println!("Size: {:?}", config.stream_config(0).size());
    println!("Pixel Format: {:?}", config.stream_config(0).pixel_format());

    assert_eq!(
        config.validate(),
        libcamera::CameraConfigurationStatus::Valid
    );

    let camera = camera.configure(&mut config)?;
    println!("Configured!");

    println!("Stride: {:?}", config.stream_config(0).stride());

    let mut frame_buffer_allocator = camera.new_frame_buffer_allocator();

    let stream_config = config.stream_config(0);
    println!("Stream: {}", stream_config.to_string());
    println!("Stream ID: {:x}", stream_config.stream().unwrap().id());

    let stream = stream_config.stream().unwrap();

    let mut frame_buffer = {
        let mut frame_buffers = frame_buffer_allocator.allocate(stream)?;

        // We only requested that one buffer be generated.
        frame_buffers.pop().unwrap()
    };

    frame_buffer.map_memory()?;

    let mut request = camera.create_request(0);
    request.add_buffer(frame_buffer)?;

    let mut controls = request.controls_mut();
    controls.set(libcamera::controls::AeEnable, true);

    // controls.set(id, value)

    println!("Request Controls: {:?}", request.controls_mut());

    let camera = camera.start(None)?;

    let mut pending_request = request.enqueue()?;

    let completed_request;
    loop {
        match pending_request.try_complete() {
            Ok(v) => {
                completed_request = v;
                break;
            }
            Err(v) => {
                pending_request = v;
                std::thread::sleep(std::time::Duration::from_millis(2));
                continue;
            }
        }
    }

    println!("Request: {}", completed_request.to_string());

    assert_eq!(
        completed_request.status(),
        libcamera::RequestStatus::RequestComplete
    );

    println!("{:?}", completed_request.status());

    let frame_buffer = completed_request.buffer(stream).unwrap();
    assert_eq!(
        frame_buffer.metadata().status,
        libcamera::FrameStatus::FrameSuccess
    );

    println!("Planes: {:?}", frame_buffer.planes());

    let used_memory = frame_buffer.used_memory().unwrap();

    println!("Response Metadata: {:?}", completed_request.metadata());

    // NOTE: These two timestamps give identical values.
    println!("Timestamp: {}", frame_buffer.metadata().timestamp);

    let t = completed_request
        .metadata()
        .get(libcamera::controls::SensorTimestamp);
    println!("Timestamp (Request): {}", t.unwrap_or(-1));

    // SensorTimestamp

    println!("Size: {}", used_memory.len());

    std::fs::write("image.bin", used_memory).unwrap();

    println!("Written!");

    return Ok(());

    std::thread::sleep(std::time::Duration::from_secs(10));
    */

    Ok(())
}
