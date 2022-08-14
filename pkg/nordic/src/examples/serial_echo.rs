use executor::define_thread;

use crate::timer::Timer;
use crate::uarte::UARTE;

use logging::num_to_slice;

// Thread that reads bytes from a UART RX pin and then writes then back to the
// TX pin.
define_thread!(
    SerialEcho,
    serial_echo_thread_fn,
    serial: UARTE,
    timer: Timer
);
async fn serial_echo_thread_fn(serial: UARTE, mut timer: Timer) {
    let mut buf = [0u8; 64];

    let (mut reader, mut writer) = serial.split();

    let mut timer2 = timer.clone();

    loop {
        let mut read = reader.begin_read(&mut buf);

        enum Event {
            DoneRead,
            Timeout,
        }

        loop {
            let e = race!(
                executor::futures::map(read.wait(), |_| Event::DoneRead),
                executor::futures::map(timer2.wait_ms(10), |_| Event::Timeout),
            )
            .await;

            match e {
                Event::DoneRead => {
                    drop(read);

                    writer.write(&buf).await;

                    // Restart the read.
                    break;
                }
                Event::Timeout => {
                    if !read.is_empty() {
                        let n = read.cancel().await;

                        writer.write(b"Read: ").await;
                        writer.write(num_to_slice(n as u32).as_ref()).await;
                        writer.write(b"\n").await;

                        writer.write(&buf[0..n]).await;
                        break;
                    }
                }
            }
        }
    }
}
