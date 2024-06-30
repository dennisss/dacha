#[macro_use]
extern crate macros;

use std::time::Duration;

use base_error::*;

#[executor_main]
async fn main() -> Result<()> {
    let mut dev = v4l2::Device::open("/dev/video2").await?;
    dev.print_capabiliites().await?;

    /*
    Listing devices:
    - Find all devices with the same bus info

    */

    // TODO: Configure the 'power_line_frequency'

    let mut capture_stream = dev.new_capture_stream()?;

    /*
    The 1080p camera I'm using is locked at around 6Mbps bitrate on H264

    */

    let formats = dev
        .list_formats(v4l2::v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_CAPTURE)
        .await?;

    for format in formats {
        println!("{:?}", format);
        println!("{:?}", dev.list_frame_sizes(format.pixelformat).await?);
    }

    return Ok(());

    {
        // TODO: Must verify the device has the streaming capability and the streaming
        // params has the timeperframe capability.

        /*
        let mut format = v4l2::v4l2_format::default();
        format.fmt.pix_mp.width = 1920 as u32;
        format.fmt.pix_mp.height = 1080 as u32;
        format.fmt.pix_mp.pixelformat = v4l2::V4L2_PIX_FMT_H264;
        format.fmt.pix_mp.field = v4l2::v4l2_field::V4L2_FIELD_ANY.0;
        format.fmt.pix_mp.colorspace = v4l2::v4l2_colorspace::V4L2_COLORSPACE_DEFAULT.0;
        format.fmt.pix_mp.num_planes = 1;
        unsafe {
            format.fmt.pix_mp.plane_fmt[0].bytesperline = 0;
            format.fmt.pix_mp.plane_fmt[0].sizeimage = 512 << 10;
        }
        */

        let mut format = capture_stream.get_format().await?;
        println!("Capture format: {}", unsafe {
            // Should be the 'H264' four-CC code
            std::str::from_utf8(&u32::to_le_bytes(format.fmt.pix_mp.pixelformat)).unwrap()
        });

        unsafe {
            println!("{:?}", format.fmt.pix.width);
            println!("{:?}", format.fmt.pix.height);
        }

        // format.fmt.pix_mp.width = 800;
        // format.fmt.pix_mp.height = 600;
        // */
        // NOTE: We must set this at least once in a device's lifetime, otherwise, it
        // may be in an invalid unconfigured state.
        capture_stream.set_format(format).await?;

        // TODO: Also enumerate supported frame intervals.

        let mut params = capture_stream.get_streaming_params().await?;
        let capture_param = unsafe { &mut params.parm.capture };

        if capture_param.capability & v4l2::V4L2_CAP_TIMEPERFRAME == 0 {
            return Err(err_msg("Device doesn't support setting the frame rate"));
        }

        capture_param.timeperframe.numerator = 1;
        capture_param.timeperframe.denominator = 30;

        capture_stream.set_streaming_params(params).await?;
    }

    println!("Good!");

    // Make memory buffers.

    let (mut capture_stream, capture_buffers) = capture_stream.configure_mmap(4).await?;

    // TODO: Verify that attempting to dequeue a capture buffer fails until it has
    // data?
    for buf in capture_buffers {
        capture_stream.enqueue_buffer(buf).await?;
    }

    capture_stream.turn_on().await?;

    let mut combined = vec![];

    for i in 0..(30) {
        let buf = capture_stream.dequeue_buffer().await?;
        // println!("Size: {} / {}", buf.used_memory().len(), buf.memory().len());

        // NOTE: 'sequence' starts at 0 for first frame and will be sequential unless we
        // miss frames.
        println!("{} : {:?}", buf.sequence(), buf.monotonic_timestamp());

        // println!("{:?}", &buf.used_memory()[0..10]);

        combined.extend_from_slice(buf.used_memory());

        capture_stream.enqueue_buffer(buf).await?;

        if i == 10 {
            executor::sleep(Duration::from_secs(1)).await?;
        }
    }

    // file::write("video.h264", &combined).await?;

    Ok(())
}
