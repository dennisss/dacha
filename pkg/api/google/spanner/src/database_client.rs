use core::ops::{Deref, DerefMut};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};
use std::{convert::TryFrom, sync::Arc};

use common::errors::*;
use executor::channel::queue::ConcurrentQueue;
use executor::child_task::ChildTask;
use executor::sync::Mutex;
use executor::Condvar;
use google_auth::*;
use http::uri::Uri;
use http::{AffinityContext, AffinityKey, AffinityKeyCache};

use googleapis_proto::google::longrunning as longrunning_proto;
use googleapis_proto::google::spanner::admin::database::v1 as admin_proto;
use googleapis_proto::google::spanner::v1 as proto;
use parsing::ascii::AsciiString;
use protobuf_builtins::google::protobuf::ListValue;

pub(crate) const PRODUCTION_TARGET: &'static str = "https://spanner.googleapis.com";

/*
- Want to create a channel which has N connections
- Create up to K sessions per connection
    - Each session should be pinned via an affinity key

TODO: Add labels to each session to reference the current cluster/container metadata.
- Must handle NOT_FOUND if using an invalid session
- Must use `SELECT 1` to keep a session alive (every 10 minutes)
- Automatically cycle to a new session if a session has been in use for more than 1 day
- Monitor the max number of sessions in use at a time so that we can calibrate this in the future.
- Limit max num active connections per channel to 1

- A channel with connection issues should stop getting used.

Across multiple databases, we can re-use connections?

TODO: Maintain a high idle timeout for the http::Client
*/

const BACKGROUND_THREAD_INTERVAL: Duration = Duration::from_secs(60);

/// If a session hasn't been used for this long, we will send a dummy request to
/// keep it alive.
const SESSION_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(10 * 60); // 10 mins

/// Basically we are pinging the sessions more frequently than this so
/// connections should never enter an idle state.
///
/// TODO: Automatically disable this for any stubs are a server dependency?
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// If sessions exceed this age, we will try to replace them with new ones.
const MAX_SESSION_AGE: Duration = Duration::from_secs(24 * 60 * 60); // 1 day

#[derive(Clone)]
pub struct SpannerDatabaseClientOptions {
    pub project_id: String,

    pub instance_name: String,

    pub database_name: String,

    pub service_account: Arc<GoogleServiceAccount>,

    pub session_count: usize,
}

/// Client for performing read/write operations on a specific Spanner database.
pub struct SpannerDatabaseClient {
    shared: Arc<Shared>,
    background_task: ChildTask,
}

struct Shared {
    options: SpannerDatabaseClientOptions,

    database_resource_path: String,

    /// TODO: Consider supporting stub re-use across many databases.
    stub: proto::SpannerStub,

    state: Condvar<State>,

    // TODO: Clear sessions from here when done.
    affinity_pins: AffinityKeyCache,
}

struct State {
    sessions: HashMap<AsciiString, Session>,

    /// Sessions which are not currently being used.
    available_sessions: HashSet<AsciiString>,
}

struct Session {
    affinity_key: AffinityKey,

    // TODO: Better to use an 'Instant' here?
    creation_time: SystemTime,

    last_used: SystemTime,

    state: SessionState,
}

#[derive(PartialEq, Eq)]
enum SessionState {
    /// Session is currently out in the wild and might be in use by a user.
    Active,

    /// Someone is currently using the session and we will finish removing this
    /// session once the user is done with it.
    ShuttingDown,

    /// Session is too old and ready to be deleted. No one is using it.
    Inactive,
}

/// Wrapper around a session to guarantee that when it is not longer in use, it
/// returns to the pool.
///
/// All operations involving a session should operate with one of these in
/// scope. should use '.name()' to get the name of the session and once an RPC
/// has finished, should use '.process_result()' to track any returned statuses.
struct SessionGuard<'a> {
    shared: &'a Arc<Shared>,
    session_name: Option<AsciiString>,
    is_dead: bool,
}

impl Drop for SessionGuard<'_> {
    fn drop(&mut self) {
        // TODO: When we are here, the RPC task might have been cancelled which means
        // that the server might still believe that the session is active. How do we
        // block until the cancellation has fully propagated through the HTTP stack.

        let shared = self.shared.clone();
        let session_name = self.session_name.take().unwrap();
        let is_dead = self.is_dead;
        executor::spawn(async move { Self::reclaim_impl(shared, session_name, is_dead).await });
    }
}

impl<'a> SessionGuard<'a> {
    fn new(shared: &'a Arc<Shared>, session_name: AsciiString) -> Self {
        Self {
            shared,
            session_name: Some(session_name),
            is_dead: false,
        }
    }

    fn name(&self) -> &str {
        self.session_name.as_ref().unwrap().as_str()
    }

    fn process_result<T>(&mut self, res: Result<T>) -> Result<T> {
        if let Err(e) = &res {
            if let Some(status) = e.downcast_ref::<rpc::Status>() {
                if let Some(resource_info) =
                    status.detail::<googleapis_proto::google::rpc::ResourceInfo>()?
                {
                    if resource_info.resource_type()
                        == "type.googleapis.com/google.spanner.v1.Session"
                    {
                        self.is_dead = true;
                    }
                }
            }
        }

        res
    }

    async fn reclaim_impl(shared: Arc<Shared>, session_name: AsciiString, is_dead: bool) {
        let mut state = shared.state.lock().await;

        if is_dead {
            state.sessions.remove(&session_name);
            return;
        }

        let session = state.sessions.get_mut(&session_name).unwrap();

        if session.state != SessionState::Active {
            session.state = SessionState::Inactive;
            return;
        }

        state.available_sessions.insert(session_name);
        state.notify_all();
    }
}

impl SpannerDatabaseClient {
    pub async fn create(options: SpannerDatabaseClientOptions) -> Result<Self> {
        let service_uri: Uri = Uri::try_from(PRODUCTION_TARGET)?;

        let creds = google_auth::GoogleServiceAccountJwtCredentials::create(
            service_uri.clone(),
            options.service_account.clone(),
        )?;

        let mut channel_options =
            rpc::Http2ChannelOptions::try_from(http::ClientOptions::from_uri(&service_uri)?)?;
        channel_options.credentials = Some(Box::new(creds));
        channel_options.http.backend_balancer.subset_size = 1;
        channel_options.http.backend_balancer.backend.idle_timeout = CONNECTION_IDLE_TIMEOUT;
        channel_options
            .http
            .backend_balancer
            .backend
            .eagerly_connect = true;
        // Increase parallelism for sharding sessions.
        channel_options.http.backend_balancer.target_parallelism = 2;

        let channel = Arc::new(rpc::Http2Channel::create(channel_options).await?);

        let stub = proto::SpannerStub::new(channel);

        let database_resource_path = format!(
            "projects/{}/instances/{}/databases/{}",
            options.project_id, options.instance_name, options.database_name
        );

        let creation_time = SystemTime::now();

        let sessions_res = {
            let mut req = proto::BatchCreateSessionsRequest::default();
            req.set_database(&database_resource_path);
            req.set_session_count(options.session_count as i32);

            let mut ctx = rpc::ClientRequestContext::default();
            ctx.http.wait_for_ready = true;
            stub.BatchCreateSessions(&ctx, &req).await.result?
        };

        let mut sessions = HashMap::new();
        let mut available_sessions = HashSet::new();
        for session in sessions_res.session() {
            let name = AsciiString::new(session.name());

            sessions.insert(
                name.clone(),
                Session {
                    affinity_key: AffinityKey::new(session.name()),
                    creation_time: creation_time.clone(),
                    last_used: creation_time.clone(),
                    state: SessionState::Active,
                },
            );

            available_sessions.insert(name);
        }

        let shared = Arc::new(Shared {
            options,
            database_resource_path,
            stub,
            affinity_pins: AffinityKeyCache::default(),
            state: Condvar::new(State {
                sessions,
                available_sessions,
            }),
        });

        let background_task = ChildTask::spawn(Self::run_background_thread(shared.clone()));

        Ok(Self {
            shared,
            background_task,
        })
    }

    // TODO: Monitor whether or not this thread is failing.
    async fn run_background_thread(shared: Arc<Shared>) {
        let mut idle_sessions = HashSet::new();
        let mut deletable_sessions = HashSet::new();

        loop {
            let now = SystemTime::now();

            // Find sessions we need to ping or refresh.
            let total_num_sessions;
            let mut num_active_sessions = 0;
            let mut num_active_old_sessions = 0;
            idle_sessions.clear();
            deletable_sessions.clear();
            {
                let mut state_guard = shared.state.lock().await;
                let state = &mut *state_guard;

                total_num_sessions = state.sessions.len();

                for (session_name, session) in state.sessions.iter() {
                    if session.state == SessionState::Active {
                        num_active_sessions += 1;

                        if session.creation_time + MAX_SESSION_AGE < now {
                            num_active_old_sessions += 1;
                        }
                    }
                }

                for (session_name, session) in state.sessions.iter_mut() {
                    if session.creation_time + MAX_SESSION_AGE < now
                        || session.state != SessionState::Active
                    {
                        // Disallow deletion of the session if we don't have at least
                        // 'session_count' other available sessions.
                        let mut can_delete = true;
                        if session.state == SessionState::Active {
                            if num_active_sessions > shared.options.session_count {
                                num_active_sessions -= 1;
                                num_active_old_sessions -= 1;
                            } else {
                                can_delete = false;
                            }
                        }

                        if can_delete {
                            // We can delete a session if no one is using it. Otherwise need to wait
                            // for the session to be marking inactive in
                            // the SessionGuard destructor.
                            if session.state == SessionState::Inactive
                                || state.available_sessions.remove(session_name)
                            {
                                session.state = SessionState::Inactive;
                                deletable_sessions
                                    .insert((session_name.clone(), session.affinity_key));
                            } else {
                                session.state = SessionState::ShuttingDown;
                            }

                            // No need to heartbeat sessions we are going to delete.
                            continue;
                        }
                    }

                    if session.last_used + SESSION_KEEP_ALIVE_INTERVAL < now {
                        idle_sessions.insert(session_name.clone());
                    }
                }

                for (session_name, _) in &deletable_sessions {
                    state.sessions.remove(session_name);
                }
            }

            // Ping sessions
            for session_name in idle_sessions.drain() {
                eprintln!("Heartbeat session: {}", session_name.as_str());

                let (mut session, mut ctx) =
                    match Self::get_session_impl(&shared, Some(session_name)).await {
                        Some(v) => v,
                        None => continue,
                    };

                ctx.http.wait_for_ready = true;

                let mut req = proto::ExecuteSqlRequest::default();
                req.set_session(session.name());
                req.set_sql("SELECT 1");

                if let Err(e) =
                    session.process_result(shared.stub.ExecuteSql(&ctx, &req).await.result)
                {
                    eprintln!("Failed to refresh idle session: {}", e);
                }
            }

            // Make more sessions if needed.
            let num_active_new_backends = num_active_sessions - num_active_old_sessions;
            if num_active_new_backends < shared.options.session_count {
                let n = shared.options.session_count - num_active_new_backends;

                println!("Make more sessions: {}", n);

                // TODO: Deduplicate with the constructor code

                let creation_time = SystemTime::now();

                let sessions_res = {
                    let mut req = proto::BatchCreateSessionsRequest::default();
                    req.set_database(&shared.database_resource_path);
                    req.set_session_count(n as i32);

                    let mut ctx = rpc::ClientRequestContext::default();
                    ctx.http.wait_for_ready = true;
                    shared.stub.BatchCreateSessions(&ctx, &req).await.result
                };

                match sessions_res {
                    Ok(sessions_res) => {
                        let mut state = shared.state.lock().await;

                        for session in sessions_res.session() {
                            let name = AsciiString::new(session.name());

                            state.sessions.insert(
                                name.clone(),
                                Session {
                                    affinity_key: AffinityKey::new(session.name()),
                                    creation_time: creation_time.clone(),
                                    last_used: creation_time.clone(),
                                    state: SessionState::Active,
                                },
                            );

                            state.available_sessions.insert(name);
                        }

                        state.notify_all();
                    }
                    Err(e) => {
                        eprintln!("Failed to create more spanner sessions: {}", e);
                    }
                }
            }

            // Delete sessions
            for (session_name, affinity_key) in deletable_sessions.drain() {
                let mut req = proto::DeleteSessionRequest::default();
                req.set_name(session_name.as_str());

                let mut ctx = rpc::ClientRequestContext::default();
                ctx.http.wait_for_ready = true;
                ctx.http.affinity = Some(AffinityContext {
                    key: affinity_key.clone(),
                    reassignment_tolerant: false,
                    cache: Some(shared.affinity_pins.clone()),
                });

                // Perform best effort session deletion.
                // TODO: Use affinity for this request.
                if let Err(e) = shared.stub.DeleteSession(&ctx, &req).await.result {
                    eprintln!(
                        "[SpannerDatabaseClient] Failed to delete old session: {}",
                        e
                    );
                }

                shared.affinity_pins.remove(key)
            }

            executor::sleep(BACKGROUND_THREAD_INTERVAL).await;
        }
    }

    async fn get_session(&self) -> (SessionGuard, rpc::ClientRequestContext) {
        Self::get_session_impl(&self.shared, None).await.unwrap()
    }

    // If given a session name, we will return None immediately if it isn't
    // available.
    async fn get_session_impl(
        shared: &Arc<Shared>,
        session_name: Option<AsciiString>,
    ) -> Option<(SessionGuard, rpc::ClientRequestContext)> {
        loop {
            let mut state = shared.state.lock().await;
            if state.available_sessions.is_empty() {
                if session_name.is_some() {
                    return None;
                }

                state.wait(()).await;
                continue;
            }

            let session_name = match session_name {
                Some(v) => v,
                None => state.available_sessions.iter().next().unwrap().clone(),
            };

            if !state.available_sessions.remove(&session_name) {
                return None;
            }

            let mut session = state.sessions.get_mut(&session_name).unwrap();

            let mut ctx = rpc::ClientRequestContext::default();
            ctx.http.affinity = Some(AffinityContext {
                key: session.affinity_key.clone(),
                // Only need to set to false if this request is part of a transaction.
                reassignment_tolerant: true,
                cache: Some(shared.affinity_pins.clone()),
            });

            // NOTE: May be inaccurate if the operation is immediately cancelled after the
            // session is retrieved.
            session.last_used = SystemTime::now();

            return Some((SessionGuard::new(shared, session_name), ctx));
        }
    }

    pub async fn insert(
        &self,
        table: &str,
        columns: &[String],
        values: &[ListValue],
    ) -> Result<()> {
        let (mut session, ctx) = self.get_session().await;

        let mut req = proto::CommitRequest::default();
        req.set_session(session.name());

        let mutation = req.new_mutations();

        // Swap to insert_or_update if that's what we want to do. Or replace()
        mutation.insert_mut().set_table(table);
        for c in columns {
            mutation.insert_mut().add_columns(c.to_string());
        }
        for v in values {
            mutation.insert_mut().add_values(v.clone());
        }

        req.single_use_transaction_mut().read_write_mut();

        session.process_result(self.shared.stub.Commit(&ctx, &req).await.result)?;
        Ok(())
    }

    pub async fn read(&self, mut req: proto::ReadRequest) -> Result<proto::ResultSet> {
        let (mut session, ctx) = self.get_session().await;

        req.set_session(session.name());

        // TODO: Verify there is no page token.
        session.process_result(self.shared.stub.Read(&ctx, &req).await.result)
    }
}
