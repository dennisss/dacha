
find_package(Boost COMPONENTS system filesystem REQUIRED)

#find_package(OpenSSL)

#link_directories(
#	/Users/dennis/Workspace/rocksdb
#)
#include_directories(
#	/Users/dennis/Workspace/rocksdb/include
#)

#include_directories(
#	lib/socket.io-client-cpp/build/include
#)

#link_directories(
#	build_socketio_cpp
#)



# https://stackoverflow.com/questions/36868143/what-is-the-recommended-way-of-using-glib2-with-cmake
find_package(PkgConfig REQUIRED)
pkg_search_module(GLIB REQUIRED glib-2.0)
#target_include_directories(mytarget PRIVATE ${GLIB_INCLUDE_DIRS})
#target_link_libraries(mytarget INTERFACE ${GLIB_LDFLAGS})

include_directories(
	#${OPENSSL_INCLUDE_DIR}
	"/usr/local/Cellar/openssl@1.1/1.1.0e/include"
	"/usr/local/Cellar/libnice/0.1.14_1/include"
	
	${GLIB_INCLUDE_DIRS}
	#"/usr/local/Cellar/glib/2.56.1/include/glib-2.0"
)

link_directories(
	"/usr/local/Cellar/openssl@1.1/1.1.0e/lib"
	"/usr/local/Cellar/libnice/0.1.14_1/lib"
	"/usr/local/Cellar/glib/2.56.1/lib"
)


add_subdirectory(msgs)


add_executable(
	main

	main.cpp
	redis_server.cpp
	redis_connection.cpp


	#nice.cpp
	#main2.cpp
	#redis.cpp
)

target_link_libraries(
	main

	rocksdb
	pthread

	ssl
	crypto
	nice
	#sioclient

	${GLIB_LDFLAGS}
	gobject-2.0.0
	gio-2.0.0
	#${OPENSSL_LIBRARIES}
	${Boost_LIBRARIES}
	
)

set_target_properties(main PROPERTIES COMPILE_FLAGS "-std=c++1y") # -O3