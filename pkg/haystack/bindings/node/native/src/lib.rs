#[macro_use] extern crate neon;

extern crate haystack;

use neon::prelude::*;
use haystack::directory::*;
use haystack::client::*;
use haystack::common::*;


declare_types! {
	pub class JsClient for Client {
		init(mut cx) {
			let dir = Directory::open().unwrap();
			Ok(Client::create(dir))
		}

		method get(mut cx) {
			let attr: String = cx.argument::<JsString>(0)?.value();

			let this = cx.this();

			match &attr[..] {
				"cluster_id" => {
					let id = {
						let guard = cx.lock();
						let client = this.borrow(&guard);
						client.cluster_id()
					};
					Ok(cx.string(id).upcast())
				},
				_ => cx.throw_type_error("property does not exist")
			}
		}

		method read_photo_cache_url(mut cx) {
			// NOTE: Possible unsafe cast from f64
			let key = cx.argument::<JsNumber>(0)?.value() as NeedleKey;
			let alt_key = cx.argument::<JsNumber>(1)?.value() as NeedleAltKey;

			let this = cx.this();
			let url = {
				let guard = cx.lock();
				let client = this.borrow(&guard);

				client.read_photo_cache_url(&NeedleKeys {
					key: 12,
					alt_key: 0
				}).unwrap()
			};

			Ok(cx.string(url).upcast())
		}

		method upload_photo(mut cx) {
			let arr_handle: Handle<JsArray> = cx.argument(0)?;
			let vec: Vec<Handle<JsValue>> = arr_handle.to_vec(&mut cx)?;

			/*
				Input [
					{ alt_key: 1, data: Buffer(..) },
					...
				]

			*/

		}

		method panic(_) {
			panic!("Client.prototype.panic")
		}
	}
}
register_module!(mut m, {
	m.export_class::<JsClient>("Client")
});
