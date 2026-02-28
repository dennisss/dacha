use std::sync::Arc;
use std::time::{Instant, Duration};

use cnc_controller_proto::cnc::*;
use common::errors::*;
use executor_multitask::{TaskResource, impl_resource_passthrough};
use executor::channel::Sender;

use crate::logging::*;
use crate::devices::*;


pub struct DataLogger {
    task: TaskResource,
    shared: Arc<Shared>
}

impl_resource_passthrough!(DataLogger, task);

struct Shared {
    config: DataLoggerConfig,
    devices: Arc<DevicesController>,
}

impl DataLogger {

    pub fn create(
        config: &DataLoggerConfig,
        devices: Arc<DevicesController>,
        logging_channel: Arc<LoggingChannel>,
    ) -> Result<Self> {

        let shared = Arc::new(Shared {
            config: config.clone(),
            devices
        });

        let task = TaskResource::spawn_interruptable(
            "DataLogger", Self::background_thread(shared.clone(), logging_channel)
        );

        Ok(Self {
            task,
            shared
        })
    }

    async fn background_thread(shared: Arc<Shared>, logging_channel: Arc<LoggingChannel>) -> Result<()> {

        if shared.config.has_ma732() {
            return Self::ma732_logger(shared, logging_channel).await;
        }

        if shared.config.has_as5047p() {
            return Self::as5047p_logger(shared, logging_channel).await;
        }

        if shared.config.has_adc() {
            return Self::adc_logger(shared, logging_channel).await;
        }

        if shared.config.has_as5601() {
            return Self::as5601_logger(shared, logging_channel).await;
        }

        Err(err_msg("Unsupported logger configuration"))
    }

    // This does basic sampling up to 100 times per second.
    async fn as5601_logger(shared: Arc<Shared>, logging_channel: Arc<LoggingChannel>) -> Result<()> {
        let config = shared.config.as5601();
        let device = shared.devices.get_peripherals_device(config.peripheral().device_name()).await?;
        
        loop {

            let mut out = [0, 0];

            let res = device.i2c_transfer(
                config.peripheral().peripheral_name(),
                0x36, // i2c address
                &[0x0C], // send data ('RAW_ANGLE' register address)
                &mut out
            ).await;

            let res = match res {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("I2C transfer failed: {}", e);
                    executor::sleep(Duration::from_millis(1000)).await?;
                    continue;
                }
            };

            let start_time = shared.devices.time().wrap_raw_time(
                config.peripheral().device_name(),
                res.transfer_time
            ).await?;            

            // This is a 12-bit value so we will make it a full 16-bit range.
            let value = u16::from_be_bytes(out) << 4;

            let mut entry = LogEntry::default();
            entry.set_logger_name(shared.config.name());
            entry.sampled_data_mut().set_start_time(
                start_time.raw());
            entry.sampled_data_mut().set_sample_interval(0u64);
            entry.sampled_data_mut().set_sample_count(1u64);
            entry.sampled_data_mut().set_data(&value.to_be_bytes()[..]);

            logging_channel.send(entry);

            executor::sleep(Duration::from_millis(10)).await?;
        }

    }

    async fn adc_logger(shared: Arc<Shared>, logging_channel: Arc<LoggingChannel>) -> Result<()> {
        let config = shared.config.adc();
        let device = shared.devices.get_peripherals_device(config.peripheral().device_name()).await?;


        let mut next_buffer_index = 0;
        let mut enqueued_requests = vec![];
        
        loop {
            while enqueued_requests.len() < config.buffers().len() {
                let buffer = &config.buffers()[next_buffer_index];

                let req = device.enqueue_analog_read_window(
                    config.peripheral().peripheral_name(),
                    &config.buffers()[next_buffer_index]
                ).await?;
                
                enqueued_requests.push((buffer, req));

                next_buffer_index = (next_buffer_index + 1) % config.buffers().len();
            }

            let (buffer, req) = enqueued_requests.remove(0);
            let res = req.await?;

            let completion_time = shared.devices.time().wrap_raw_time(
                config.peripheral().device_name(),
                res.sampling_completion_time
            ).await?;

            // This should contain i16 values (little endian)
            let data = device.fetch_buffer(buffer).await?;

            let sample_count = data.len() / 2;
            let sample_rate = config.sample_rate();
            let sample_interval = (16_000_000 / sample_rate) as u64;
            
            let start_time = completion_time.sub_ticks_u64(sample_interval * (sample_count as u64));

            let mut entry = LogEntry::default();
            entry.set_logger_name(shared.config.name());
            entry.sampled_data_mut().set_start_time(
                start_time.raw());
            // TODO: This needs to be skew corrected.
            entry.sampled_data_mut().set_sample_interval(sample_interval);
            entry.sampled_data_mut().set_sample_count(sample_count as u64);
            entry.sampled_data_mut().set_data(data);

            logging_channel.send(entry);
        }
    }

    async fn as5047p_logger(shared: Arc<Shared>, logging_channel: Arc<LoggingChannel>) -> Result<()> {
        let config = shared.config.as5047p();
        let device = shared.devices.get_peripherals_device(config.spi_peripheral().device_name()).await?;

        // TODO: Its probably not necessary for me to specify a time and instead just have it go as fast as possible between requests.
        let mut next_time = shared.devices.time().to_device_time(
            config.spi_peripheral().device_name(),
            Instant::now() + Duration::from_millis(200)
        ).await?;

        let mut enqueued_requests = vec![];
        let mut next_buffer_idx = 0;

        let read_request = crate::as5047p::create_as5047p_command(crate::as5047p::ANGLECOM /* ANGLEUNC */, true); 

        loop {
            while enqueued_requests.len() < config.buffers().len() {

                let buf = &config.buffers()[next_buffer_idx % config.buffers().len()];
                next_buffer_idx += 1;

                let sample_rate = 8000;

                // Note that buffers currently store 8000 entries.
                let transfer_count = 8000;
                let transfer_interval = 16_000_000 / (sample_rate as u32);

                let mut entry = LogEntry::default();
                entry.set_logger_name(shared.config.name());
                entry.sampled_data_mut().set_start_time(
                    shared.devices.time().to_primary_clock(next_time).await?.raw());
                // TODO: This needs to be skew corrected.
                entry.sampled_data_mut().set_sample_interval(transfer_interval as u64);
                entry.sampled_data_mut().set_sample_count(transfer_count as u64);

                enqueued_requests.push((
                    entry,
                    buf,
                    device.enqueue_spi_transfer_timed(
                        config.spi_peripheral().peripheral_name(),
                        &read_request,
                        buf,
                        next_time.lower(),
                        transfer_count,
                        transfer_interval,
                    ).await?
                ));

                next_time = next_time.add_ticks_u64(((transfer_count as u64) + 1) * (transfer_interval as u64));
            }

            let (mut entry, buf, req) = enqueued_requests.remove(0);
            req.await?;

            let data = device.fetch_buffer(buf).await?;

            let mut processed_data = vec![];
            processed_data.reserve_exact(data.len() - 2);

            // NOTE: We must discard the first sample as it contains data from the previous transfer
            // (basically whenever we send a transfer, that should fill the output register for the next transfer to read.)
            {
                let mut bad = false;

                for i in 1..(data.len() / 2) {
                    let angle = match crate::as5047p::parse_as5047p_data(array_ref!(data, 2*i, 2)) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Bad sample!! {}", e);
                            bad = true;
                            break;
                        }
                    };

                    // Convert from 14 bit to 16 bit
                    processed_data.extend_from_slice(&(angle << 2).to_be_bytes());
                }

                if bad {
                    continue;
                }
            }

            entry.sampled_data_mut().set_sample_count((processed_data.len() / 2) as u64);
            entry.sampled_data_mut().set_data(processed_data);


            logging_channel.send(entry);
        }

    }

    async fn ma732_logger(shared: Arc<Shared>, logging_channel: Arc<LoggingChannel>) -> Result<()> {

        /*
        TODO: Need sensor configuration.
        */


        let config = shared.config.ma732();
        let device = shared.devices.get_peripherals_device(config.spi_peripheral().device_name()).await?;

        // TODO: Its probably not necessary for me to specify a time and instead just have it go as fast as possible between requests.
        let mut next_time = shared.devices.time().to_device_time(
            config.spi_peripheral().device_name(),
            Instant::now() + Duration::from_millis(200)
        ).await?;

        let mut enqueued_requests = vec![];
        let mut next_buffer_idx = 0;

        loop {
            while enqueued_requests.len() < config.buffers().len() {

                let buf = &config.buffers()[next_buffer_idx % config.buffers().len()];
                next_buffer_idx += 1;

                println!("Enqueue {}", buf);

                // Current peak is 8000.
                let transfer_count = 1000;
                let transfer_interval = 16_000_000 / 1000;

                // TODO: Need the time converted to the main 

                let mut entry = LogEntry::default();
                entry.set_logger_name(shared.config.name());
                entry.sampled_data_mut().set_start_time(
                    shared.devices.time().to_primary_clock(next_time).await?.raw());
                // TODO: This needs to be skew corrected.
                entry.sampled_data_mut().set_sample_interval(transfer_interval as u64);
                entry.sampled_data_mut().set_sample_count(transfer_count as u64);

                enqueued_requests.push((
                    entry,
                    buf,
                    device.enqueue_spi_transfer_timed(
                        config.spi_peripheral().peripheral_name(),
                        &[0, 0],
                        buf,
                        next_time.lower(),
                        transfer_count,
                        transfer_interval,
                    ).await?
                ));

                next_time = next_time.add_ticks_u64(((transfer_count as u64) + 1) * (transfer_interval as u64));
            }

            let (mut entry, buf, req) = enqueued_requests.remove(0);
            req.await?;

            let data = device.fetch_buffer(buf).await?;

            entry.sampled_data_mut().set_data(data);

            logging_channel.send(entry);
        }
    }

}
