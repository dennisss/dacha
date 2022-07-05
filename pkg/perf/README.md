

Linux Perf tracing:
- sys_perf_event_open

/*
	6 => "cycles_sample"
	6 => "count"
	6 => "cycles_event"

    PERF_COUNT_HW_CPU_CYCLES
*/

- Frame pointers

- https://www.brendangregg.com/perf.html


/*

perf record ./target/debug/perf
perf_to_profile -i perf.data -o perf.pb
pprof -web target/debug/perf perf.pb

cargo run --bin proto_viewer -- perf.pb --proto_file=third_party/google/src/proto/profile.proto --proto_type=perftools.profiles.Profile | less

*/