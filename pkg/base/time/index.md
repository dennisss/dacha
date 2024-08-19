


Standard `Time` type:

- Stores time as a number of (seconds + nanoseconds) since UNIX epoch.
- Sequential calls to `Time::now()` are guaranteed to return monotonic values or an error.
- `Time::now()` slowly converges to 'system clock time' (e.g. from NTP) and will always be within 30 seconds of the system clock.
- Specifically `Time::now()` always advances at the speed of the Linux `CLOCK_BOOTTIME` +/- 0.01%