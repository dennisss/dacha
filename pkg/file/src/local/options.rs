// TODO: Most of these are only available on linux


pub struct LocalFileOpenOptions {
    pub(super) read: bool,
    pub(super) write: bool,
    pub(super) create: bool,
    pub(super) create_new: bool,
    pub(super) sync_on_flush: bool,
    pub(super) truncate: bool,
    pub(super) append: bool,
    pub(super) sync: bool,
    pub(super) exclusive: bool,

    pub(super) direct: bool,

    pub(super) non_blocking: bool,

    /// Used when creating new files. Some bits may get masked out by 'umask'.
    pub(super) mode: u32,
}

impl LocalFileOpenOptions {
    pub fn new() -> Self {
        Self {
            read: true,
            write: false,
            create: false,
            create_new: false,
            sync_on_flush: false,
            truncate: false,
            append: false,
            direct: false,
            sync: false,
            exclusive: false,
            non_blocking: false,
            mode: 0o666,
        }
    }

    pub fn read(&mut self, value: bool) -> &mut Self {
        self.read = value;
        self
    }

    pub fn direct(&mut self, value: bool) -> &mut Self {
        self.direct = value;
        self
    }

    pub fn write(&mut self, value: bool) -> &mut Self {
        self.write = value;
        self
    }

    pub fn create(&mut self, value: bool) -> &mut Self {
        self.create = value;
        self
    }

    pub fn create_new(&mut self, value: bool) -> &mut Self {
        self.create_new = value;
        self
    }

    pub fn sync(&mut self, value: bool) -> &mut Self {
        self.sync = value;
        self
    }

    pub fn exclusive(&mut self, value: bool) -> &mut Self {
        self.exclusive = value;
        self
    }

    pub fn non_blocking(&mut self, value: bool) -> &mut Self {
        self.non_blocking = value;
        self
    }

    /// Normally when flush() is called, it will unblock when all written data
    /// has been transferred out of the current process. But if this is set to
    /// true, it will also wait for the data to be durably written to disk (or
    /// whatever the final destination is for the filesystem).
    pub fn sync_on_flush(&mut self, value: bool) -> &mut Self {
        self.sync_on_flush = value;
        self
    }

    pub fn truncate(&mut self, value: bool) -> &mut Self {
        self.truncate = value;
        self
    }

    pub fn append(&mut self, value: bool) -> &mut Self {
        self.append = value;
        self
    }
}
