#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate error_chain;

extern crate futures_await as futures;

extern crate rand;
extern crate serde;
extern crate rmp_serde as rmps;
extern crate hyper;
extern crate tokio;



static TEXT: &str = "Hello, World!";

fn main() {
	let addr = ([127, 0, 0, 1], 3000).into();



	let new_svc = || {
		service_fn_ok(|_req|{
			Response::new(Body::from(TEXT))
		})
	};

	let server = Server::bind(&addr)
		.serve(new_svc)
		.map_err(|e| eprintln!("server error: {}", e));

	hyper::rt::run(server);
}