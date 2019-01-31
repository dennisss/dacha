

include_directories(
	"/usr/local/Cellar/rocksdb/5.13.4/include"
)

link_directories(
	"/usr/local/Cellar/rocksdb/5.13.4/lib"
)

add_executable(
	main

	main.cpp
	redis_server.cpp
	redis_connection.cpp
)

target_link_libraries(
	main

	rocksdb
	pthread
)

set_target_properties(main PROPERTIES COMPILE_FLAGS "-std=c++1y -O3")

