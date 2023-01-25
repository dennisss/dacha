# USB Protocol Library

This package contains USB descriptor definitions and a host controller implementation for Linux.

## Linux Host Support

We provide support for building drivers that interact with connected USB devices in Linux. All the
packages which use USB devices should define them in the central `udev.rules` file in this
directory. To set up permissions for all devices, run the following commands:

```bash
sudo cp pkg/usb/udev.rules /etc/udev/rules.d/80-dacha.rules
sudo udevadm control --reload-rules
```

When writing driver code, a device can be discovered and opened like in the below example:

```rust
let ctx = usb::Context::create()?;

// Open based on a vendor and product id.
let mut dev = ctx.open_device(0x8888, 0x0001).await?;

dev.reset()?;

dev.write_interrupt(0x02, b"ABC").await?;

let mut data = [0u8; 256];
let n = dev.read_interrupt(0x81, &mut data).await?;

println!("Num bytes read: {}", n);
println!("{:?}", &data[0..n]);
```