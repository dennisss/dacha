// This file contains the protocol definitions/utilities used for the 'post_message' style
// JSON IPC between the web UI and the Rust code.

use common::errors::*;

#[derive(Parseable, Debug)]
pub struct WebMessage {
    pub start_rpc: Option<StartRpcMessage>,
    pub cancel_rpc: Option<CancelRpcMessage>,
}

#[derive(Parseable, Debug)]
pub struct StartRpcMessage {
    pub service_name: String,
    pub method_name: String,
    pub request: String,
    pub request_id: usize,
}

#[derive(Parseable, Debug)]
pub struct CancelRpcMessage {
    pub request_id: usize,
}


impl WebMessage {
    pub fn build_data_response(request_id: usize, json_data: &str) -> String {
        format!(
            "{{\"rpc_response\":{{\"request_id\":{},\"data\":{}}}}}",
            request_id,
            json_data
        )
    }

    pub fn build_result_response(request_id: usize, res: Result<()>) -> String {
        let status = match res {
            Ok(()) => {
                rpc::Status::ok()
            }
            Err(e) => {
                eprintln!("[webview rpc] RPC Error: {}", e);
                match e.downcast_ref::<rpc::Status>() {
                    Some(s) => {
                        if s.local() {
                            s.clone()
                        } else {
                            rpc::Status::internal("Internal error occurred")
                        }
                    },
                    None => rpc::Status::internal("Internal error occurred"),
                }
            }
        };

        Self::build_status_response(request_id, &status)
    }

    pub fn build_status_response(request_id: usize, status: &rpc::Status) -> String {
        let mut stringifier = json::Stringifier::new(json::StringifyOptions::default());
        let mut root = stringifier
            .root_value().object();
        let mut obj = root.key("rpc_response").object();

        obj.key("request_id").number(request_id as f64);
        
        let mut s = obj.key("status").object();
        s.key("code").number(status.code().to_value() as f64);
        s.key("message").string(status.message());
        drop(s);

        drop(obj);
        drop(root);

        stringifier.finish()
    }

}