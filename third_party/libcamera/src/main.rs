use libcamera::Result;

fn main() -> Result<()> {
    let manager = libcamera::CameraManager::create()?;

    let mut cameras = manager.cameras();

    println!("Num Cameras: {}", cameras.len());

    if cameras.len() == 0 {
        return Ok(());
    }

    // TOOD: Ignore ones on Pi that contain "/usb"
    let camera = cameras.pop().unwrap();
    println!("Id: {}", camera.id());

    println!("Static Num Streams: {}", camera.streams().len());
    for stream in camera.streams() {
        println!("S: {:x}", stream.id())
    }

    println!("{:#?}", camera.controls());

    let camera = camera.acquire()?;
    println!("Acquired!");

    let mut config = camera
        .generate_configuration(&[libcamera::StreamRole::Viewfinder])
        .unwrap();
    assert_eq!(config.stream_configs_len(), 1);

    // Only allocate one buffer per stream.
    config.stream_config_mut(0).set_buffer_count(1);

    println!("Supported Formats:");
    for format in config.stream_config(0).formats().pixel_formats() {
        println!("- {:?}", format);
    }

    println!("Size: {:?}", config.stream_config(0).size());
    println!("Pixel Format: {:?}", config.stream_config(0).pixel_format());

    assert_eq!(
        config.validate(),
        libcamera::CameraConfigurationStatus::Valid
    );

    let camera = camera.configure(&mut config)?;
    println!("Configured!");

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

    let camera = camera.start()?;

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

    let used_memory = frame_buffer.used_memory().unwrap();

    println!("Timestamp: {}", frame_buffer.metadata().timestamp);
    println!("Size: {}", used_memory.len());

    std::fs::write("image.jpeg", used_memory).unwrap();

    println!("Written!");

    return Ok(());

    std::thread::sleep(std::time::Duration::from_secs(10));

    Ok(())
}
