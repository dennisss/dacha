/// Set of all descriptors exposed by a USB device.
///
/// This is normally used on the device's MCU and is stored in flash to be sent
/// to the host on request. Rather than implementing this manually, you should
/// prefer to use the DescriptorSetBuilder to automatically construct this.
pub trait DescriptorSet {
    /// Retrieves the serialized value of the DeviceDescriptor for this device.
    fn device_bytes(&self) -> &[u8];

    /// Retrieves the serialized value of a ConfigurationDescriptor associated
    /// with this device in addition to all descriptors associated with it.
    fn config_bytes(&self, index: u8) -> Option<&[u8]>;

    fn string_bytes(&self, index: u8) -> Option<&[u8]>;
}
