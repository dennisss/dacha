use core::future::Future;

use usb::descriptors::{DescriptorType, SetupPacket, StandardRequestType};
use usb::DescriptorSet;

use crate::usb::controller::{
    USBDeviceControlRequest, USBDeviceControlResponse, USBDeviceNormalRequest,
};
use crate::usb::handler::{USBDeviceHandler, USBError};

/// Default USB packet handler which implements retrieval of descriptors for a
/// device with a single static configuration.
pub struct USBDeviceDefaultHandler<D> {
    descriptors: D,
}

impl<D: DescriptorSet + 'static> USBDeviceHandler for USBDeviceDefaultHandler<D> {
    type HandleControlRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleControlResponseFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalRequestFuture<'a> = impl Future<Output = Result<(), USBError>> + 'a;

    type HandleNormalResponseAcknowledgedFuture<'a> =
        impl Future<Output = Result<(), USBError>> + 'a;

    fn handle_control_request<'a>(
        &'a mut self,
        setup: SetupPacket,
        req: USBDeviceControlRequest<'a>,
    ) -> Self::HandleControlRequestFuture<'a> {
        self.handle_control_request_impl(setup, req)
    }

    fn handle_control_response<'a>(
        &'a mut self,
        setup: SetupPacket,
        res: USBDeviceControlResponse<'a>,
    ) -> Self::HandleControlResponseFuture<'a> {
        self.handle_control_response_impl(setup, res)
    }

    fn handle_normal_request<'a>(
        &'a mut self,
        endpoint_index: usize,
        req: USBDeviceNormalRequest,
    ) -> Self::HandleNormalRequestFuture<'a> {
        async move { Ok(()) }
    }

    fn handle_normal_response_acknowledged<'a>(
        &'a mut self,
        endpoint_index: usize,
    ) -> Self::HandleNormalResponseAcknowledgedFuture<'a> {
        async move { Ok(()) }
    }
}

impl<D: DescriptorSet + 'static> USBDeviceDefaultHandler<D> {
    pub fn new(descriptors: D) -> Self {
        Self { descriptors }
    }

    async fn handle_control_request_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut req: USBDeviceControlRequest<'a>,
    ) -> Result<(), USBError> {
        if setup.bRequest == StandardRequestType::SET_ADDRESS as u8 {
            // Don't need to do anything as this is implemented in hardware.
            return Ok(());
        } else if setup.bRequest == StandardRequestType::SET_CONFIGURATION as u8 {
            if setup.bmRequestType != 0b00000000 {
                req.stale();
                return Ok(());
            }

            // TODO: upper byte of wValue is reserved.
            // TODO: Value of 0 puts device in address state.

            if setup.wValue != 1 {
                req.stale();
                return Ok(());
            }

            // No data stage

            // Status stage
            // TODO: This is standard from any 'Host -> Device' request
            req.read(&mut []).await?;
            // self.periph.tasks_ep0status.write_trigger();
        } else {
            req.stale();
        }

        Ok(())
    }

    async fn handle_control_response_impl<'a>(
        &'a mut self,
        setup: SetupPacket,
        mut res: USBDeviceControlResponse<'a>,
    ) -> Result<(), USBError> {
        if setup.bRequest == StandardRequestType::GET_CONFIGURATION as u8 {
            if setup.bmRequestType != 0b10000000
                || setup.wValue != 0
                || setup.wIndex != 0
                || setup.wLength != 1
            {
                res.stale();
                return Ok(());
            }

            res.write(&[1]).await?;
        } else if setup.bRequest == StandardRequestType::GET_DESCRIPTOR as u8 {
            if setup.bmRequestType != 0b10000000 {
                res.stale();
                return Ok(());
            }

            let desc_type = (setup.wValue >> 8) as u8;
            let desc_index = (setup.wValue & 0xff) as u8; // NOTE: Starts at 0

            if desc_type == DescriptorType::DEVICE as u8 {
                if desc_index != 0 {
                    res.stale();
                    return Ok(());
                }
                // TODO: Assert language code.

                res.write(self.descriptors.device_bytes()).await?;
            } else if desc_type == DescriptorType::CONFIGURATION as u8 {
                // TODO: Validate that the configuration exists.
                // If it doesn't return an error.

                let data = match self.descriptors.config_bytes(desc_index) {
                    Some(v) => v,
                    None => {
                        res.stale();
                        return Ok(());
                    }
                };

                res.write(data).await?;
            } else if desc_type == DescriptorType::ENDPOINT as u8 {
                res.stale();
            } else if desc_type == DescriptorType::DEVICE_QUALIFIER as u8 {
                // According to the USB 2.0 spec, a full-speed only device should respond to
                // a DEVICE_QUALITY request with an error.
                //
                // TODO: Probably simpler to just us the USB V1 in the device descriptor?
                res.stale();
            } else if desc_type == DescriptorType::STRING as u8 {
                let data = match self.descriptors.string_bytes(desc_index) {
                    Some(v) => v,
                    None => {
                        res.stale();
                        return Ok(());
                    }
                };

                res.write(data).await?;
            } else {
                res.stale();
            }
        } else {
            res.stale();
        }

        Ok(())
    }
}
