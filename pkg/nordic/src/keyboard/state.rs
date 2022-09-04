pub struct KeyboardState {
    /// Number of milliseconds to wait
    pub idle_timeout: usize,

    pub protocol: KeyboardUSBProtocol,
}

enum_def_with_unknown!(KeyboardUSBProtocol u8 =>
    Boot = 0,
    Report = 1
);
