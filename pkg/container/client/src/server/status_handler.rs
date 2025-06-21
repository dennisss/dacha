use std::collections::HashMap;

use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use container_proto::cluster::*;
use executor_multitask::ServiceResource;
use http::ServerHandler;
use rpc_util::AddReflection;
use http_util::{internal_server_error, bad_request, not_found, forbidden};

use crate::credentials::get_http_server_peer_identity;
use crate::meta::client::ClusterMetaClient;
use crate::server::acl::*;
use crate::server::router::PathRouter;
use crate::service::address::ServiceName;

use super::ClusterServerHandlerData;

pub(super) struct StatusHandler {
    zone: String,
    route_paths: Vec<String>, 
}

impl StatusHandler {
    pub fn new(zone: &str, route_paths: Vec<String>) -> Self {
        Self {
            zone: zone.to_string(),
            route_paths
        }
    }

    async fn handle_request_impl<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        let cluster_context =
            match ClusterServerHandlerData::from_http_context(context.connection_context) {
                Ok(v) => v,
                Err(_) => return internal_server_error(),
            };


        /*
        TODO: Things to display:
        - General server info:
            - Binary/bundle name
            - Binary version
            - Current time
            - IP Address : Port
            - Links to other pages for liveness, rpc stats, metrics, profiling, etc.

        - ACLs
        - Stubs/channels
        - Service Resources

        TODO: Need to use 'encode_html_text' for escaping HTML

        */

        let worker_name = std::env::var(crate::env::WORKER_NAME_ENV_VAR)
            .unwrap_or_else(|_| "N/A".to_string());
        let node_id = std::env::var(crate::env::NODE_ID_ENV_VAR)
            .unwrap_or_else(|_| "N/A".to_string());

        let peer_name = match &cluster_context.peer {
            Some(v) => v.to_string(),
            None => "unauthenticated".to_string()
        };
        
        let mut routes = String::new();
        for p in &self.route_paths {
            routes.push_str(p.as_str());
            routes.push_str("<br />");
        }

        let page = format!(
            r#"
            <!doctype html>
            <html lang="en">
                <head>
                    <meta charset="utf-8">
                    <meta name="viewport" content="width=device-width, initial-scale=1">
                    <link href="/assets/node_modules/bootstrap/dist/css/bootstrap.min.css" type="text/css" rel="stylesheet">
                    <link href="/assets/pkg/web/style.css" type="text/css" rel="stylesheet">

                    <title>Cluster Server</title>
                    <style>
                        table {{
                            border-spacing: 0;
                        }}

                        td {{
                            border-left: 1px solid #ccc;
                            border-top: 1px solid #ccc;
                            padding: 5px;
                        }}

                        tr > td:last-of-type {{
                            border-right: 1px solid #ccc;
                        }}

                        tbody > tr:last-of-type > td {{
                            border-bottom: 1px solid #ccc;
                        }}
                    </style>
                </head>
                <body>
                    
                    <div style="padding: 10px">
                        <h1>Cluster Server</h1>
                        <table>
                            <tbody>
                                <tr>
                                    <td>Zone</td>
                                    <td>{zone}</td>
                                </tr>
                                <tr>
                                    <td>Worker Name</td>
                                    <td>{worker_name}</td>
                                </tr>
                                <tr>
                                    <td>Node Id</td>
                                    <td>{node_id}</td>
                                </tr>
                                <tr>
                                    <td>Peer Identity</td>
                                    <td>{peer_name}</td>
                                </tr>
                                <tr>
                                    <td>HTTP Routes</td>
                                    <td>{routes}</td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </body>
            </html>
            "#,
            zone = &self.zone,
            worker_name = worker_name,
            node_id = node_id,
            peer_name = peer_name,
            routes = routes,
        );


        http::ResponseBuilder::new()
            .status(http::status_code::OK)
            .header(http::header::CONTENT_TYPE, "text/html")
            .body(http::BodyFromData(page))
            .build()
            .unwrap()
    }
}

#[async_trait]
impl http::ServerHandler for StatusHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        self.handle_request_impl(request, context).await
    }
}
