# Peripherals

## Standard Naming

To refer to SPI/I2C host-side interfaces which **initiate** connections to other devices, we use the following terms:

- `Controller`: Interface which supports starting read/write transactions to any connected device.
    - Each unique host implementation will define `(I2C|SPI)Host` **struct**s which implement the `I2C/SPIHostController` **trait**.
- `Endpoint`: Interface for connecting with a single remote device attached to a `Controller`.
    - e.g. This will refer to an I2C device with a specific address or a SPI device with a specific CS local pin selected.
    - Each unique host implementation will define a `(I2C|SPI)HostDevice` struct which implements the `(I2C|SPI)HostEndpoint` trait.

To refer to SPI/I2C device-side interfaces which receive and respond to messages initiated from the host, we use the following terms:

- `(I2C|SPI)Device(Controller)?` this is some **struct** that implements the protocol and does the reading from the host. 
- `(I2C|SPI)DeviceHandler` is a **trait** implemented for non-device specific receivers (/ request handlers) of data received from the host. The handlers are called by the DeviceController instance to process bytes received.
