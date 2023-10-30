extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
extern crate http;
extern crate protobuf;
#[macro_use]
extern crate macros;
extern crate rpc;
extern crate web;
#[macro_use]
extern crate file;

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use common::errors::*;
use executor::bundle::TaskResultBundle;
use google_auth::GoogleServiceAccount;
use google_spanner::ProtobufTableKey;
use study_proto::study::User;

// use crate::proto::data::*;

/*
#[derive(Clone)]
struct MetricServiceImpl {
    metric_store: Arc<MetricStore>,
}

#[async_trait]
impl MetricService for MetricServiceImpl {
    async fn Query(
        &self,
        request: rpc::ServerRequest<QueryRequest>,
        response: &mut rpc::ServerResponse<QueryResponse>,
    ) -> Result<()> {
        // TODO: Validate that the start/end timestamps are non-zero and look sane (not
        // too far apart)

        let mut values = self
            .metric_store
            .query(
                request.metric_name(),
                request.start_timestamp(),
                request.end_timestamp(),
            )
            .await?;

        // Change from descending time order to ascending time order.
        values.reverse();

        let mut line = QueryResponse_Line::default();
        line.set_name("Main");

        for value in values {
            let mut point = QueryResponse_Point::default();
            point.set_timestamp(value.timestamp);
            point.set_value(value.float_value);
            line.add_points(point);
        }

        response.add_lines(line);

        Ok(())
    }
}
*/

/*
First task:

- You go to dacha.dev
    - 301 redirects to `www.dacha.dev`
- See just a portal page with a login button
- Click the button to start oauth
- Goes to `www.dacha.dev/auth/google/oauth2_begin?return_url=https://` which redirects to Google with the right state set.
- Eventually returned to www.dacha.dev/auth/google/oauth2_callback?
    - Server verifies it and sets the cookies 'HttpOnly Secure Domain=.dacha.dev'
-

- Normally [domain].dacha.dev/api/ is used for gRPC endpoints.
    - TODO: Verify that we are using a standard protobuf json format.
    - Will need to check this against a javascript/typescript style guide

Generally the server will authenticate that every request path is either allowlisted or allowed based on user id.


Cookie Usage
- X-Client-Id
- X-Session-Key

dacha.dev/blog/


Some other important things:
- HTTP2 pings
    - One per hour
- If server receives them more than

*/

pub struct UserTable {}

impl google_spanner::ProtobufTableTag for UserTable {
    type Message = study_proto::study::User;

    fn table_name(&self) -> &str {
        "User"
    }

    fn indexed_keys(&self) -> Vec<ProtobufTableKey> {
        vec![
            ProtobufTableKey {
                index_name: None,
                fields: vec![User::ID_FIELD_NUM],
            },
            ProtobufTableKey {
                index_name: Some("UserByEmailAddress".into()),
                fields: vec![User::EMAIL_ADDRESS_FIELD_NUM],
            },
        ]
    }
}

pub const USER_TABLE_TAG: UserTable = UserTable {};

pub async fn run() -> Result<()> {
    let data =
        file::read_to_string("/home/dennis/.credentials/dacha-main-748d2acba112.json").await?;

    let sa: Arc<GoogleServiceAccount> =
        Arc::new(google_auth::GoogleServiceAccount::parse_json(&data)?);

    let options = google_spanner::SpannerDatabaseClientOptions {
        project_id: "dacha-main".to_string(),
        instance_name: "instance-1".to_string(),
        database_name: "study".to_string(),
        service_account: sa,
        session_count: 2,
    };

    let client = google_spanner::SpannerDatabaseClient::create(options.clone()).await?;

    let table = google_spanner::ProtobufTable::new(&client, &USER_TABLE_TAG);

    /*
    {
        let pusher = google_spanner::SpannerDatabaseSchemaPusher::create(options.clone()).await?;

        let mut target_statements = vec![];
        target_statements.extend_from_slice(&table.data_definitions()?[..]);

        let diff = pusher.diff(&target_statements).await?;

        // TODO: Allow the user the opportunity to confirm/reject the diff.
        println!("{:#?}", diff);

        pusher.push(diff).await?;

        return Ok(());
    }
    */

    let mut user = study_proto::study::User::default();
    user.set_id(123);
    // user.set_first_name("Dennis");
    // user.set_last_name("Shtatnov");
    // user.set_email_address("densht@gmail.com");

    let users = table.read(&[User::ID_FIELD_NUM], &user).await?;
    println!("{:?}", users);

    loop {
        executor::sleep(Duration::from_secs(10)).await;
    }

    // table.insert(&[user]).await?;

    /*
    let store = Arc::new(MetricStore::open("/tmp/metricstore").await?);

    executor::spawn(collect_random(store.clone()));

    let mut task_bundle = TaskResultBundle::new();

    task_bundle.add("WebServer", {
        let web_handler = web::WebServerHandler::new(web::WebServerOptions {
            pages: vec![web::WebPageOptions {
                title: "Sensor Monitor".into(),
                path: "/".into(),
                script_path: "built/pkg/app/sensor_monitor/web.js".into(),
                vars: None,
            }],
        });

        let web_server = http::Server::new(web_handler, http::ServerOptions::default());

        web_server.run(8000)
    });

    task_bundle.add("RpcServer", {
        let mut rpc_server = rpc::Http2Server::new();
        rpc_server.add_service(
            MetricServiceImpl {
                metric_store: store.clone(),
            }
            .into_service(),
        )?;
        rpc_server.enable_cors();
        rpc_server.allow_http1();
        rpc_server.run(8001)
    });

    task_bundle.join().await

    */

    Ok(())
}
