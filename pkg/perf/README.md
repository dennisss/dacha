

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


TODO: Need to docuemtn project wide if we need to adjust the system's paranoid counter settings.



perf record -F 99 ./target/release/metastore --dir=/tmp/meta1 --init_port=4000 --port=4001


rm perf.data perf.pb

perf record target/release/rpc_transfer_benchmark

perf record ./target/debug/perf


perf record --pid=1817497


perf_to_profile -i perf.data -o perf.pb

pprof -web target/debug/metastore perf.pb

cargo run --bin proto_viewer -- perf.pb --proto_file=third_party/google/src/proto/profile.proto --proto_type=perftools.profiles.Profile | less

*/


//

curl --http2 --http2-prior-knowledge http://127.0.0.1:5002/profilez > perf.pb

pprof -web target/debug/metastore perf.pb


datastore-ca4c1

curl --http2 --http2-prior-knowledge http://127.0.0.1:8000/profilez > perf.pb

pprof -web /home/dennis/workspace/dacha/target/debug/deps/datastore-ca4c11033e862057 perf.pb


curl  --http2 --http2-prior-knowledge  http://10.1.1.1:30000/profilez > perf-meta.pb

pprof -web target/debug/cnc_monitor perf-meta.pb


curl --insecure https://127.0.0.1:10400/profilez > perf.pb