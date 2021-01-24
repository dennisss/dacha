pub enum FanControllerPacketType {
    /*
    Main protocol race condition to deal with:
    - Host 1 sends request packet to device
    - Host 2 connects
    - Device sends response to host 2 (but host 2 doesn't know about this request)
    - This is a problem if host 2 sends a request right before the response and gets a false response.
    ^ The simple way to fix this is to have the host perform a device reset (which should restart all threads)


    Commands (received from a companion program)
    - Can be over SPI or USB (both should have identical functionality and protocol)
    - GET_OPTIONS
    - SET_OPTIONS
        - Blocks until commited to EEPROM (maybe do this asyncronously to avoid wearing out EEPROM)
    - PRESS_POWER (for N milliseconds)
    - PRESS_RESET (for N milliseconds)
    - NOTE: At most one command is allowed to be executing at a time

    Events (sent to compantion program periodically)
    - STATE: Copy of in-memory state (basically struct dump)
    - LOG

        */
    /// This packet is part of the response to a host request.
    /// At most one request is allowed to be running at a time, so matching this
    /// to the request should be trivial.
    ///
    /// NOTE: Every request sent by the host will have at least one packet
    /// returned as a response. This response could be one or more Response
    /// packets or an ErrorResponse packet.
    ///
    /// Direction: Device -> Host
    Response = 0,

    /// Might be sent any time before the last Response packet in response to a
    /// request in order to indicate that an error occured during the request.
    ///
    /// The payload is an optional error message of the same type as a LogOutput
    /// message.
    ///
    /// Direction: Device -> Host
    ErrorResponse = 1,

    /// Contains a FanControllerState struct as the payload representing the
    /// latest sampled state. Periodically sent from the device after each
    /// sampling cycle.
    ///
    /// NOTE: Because the device firmwire does not buffer these, this packet
    /// will always be self contained in exactly one packet.
    ///
    /// Direction: Device -> Host
    StateSnapshot = 2,

    /// Plain text (UTF-8) log data produced by the device.
    ///
    /// Direction: Device -> Host
    LogOutput = 3,

    /// Request to get the current settings used by the device.
    ///
    /// No Request Payload. Device should response with one or more Response
    /// packets containing a FanControllerSettings struct.
    ///
    /// Direction: Host -> Device
    GetSettings = 4,

    /// Request to set the current settings used by the device.
    ///
    /// Request payload should contain the new FanControllerSettings struct. The
    /// entire request payload must be sent within 2 seconds. The response
    /// payload is empty.
    ///
    /// Direction: Host -> Device
    SetSettings = 5,

    /// Request to hold down the connected computer's power button for N
    /// milliseconds.
    ///
    /// Request payload is a 2-byte u16 representing N in little endian.
    /// Response payload will be empty.
    ///
    /// Direction: Host -> Device
    PressPower = 6,

    /// Same as PressPower but for the computer's reset button.
    PressReset = 7,
}
