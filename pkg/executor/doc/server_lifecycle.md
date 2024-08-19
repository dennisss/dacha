# Server Lifecycle Management

Most complex applications or servers will have several concurrent tasks running in order to perform all work. It is desirable to ensure that tasks are well tracked in terms of health/failures/cancellation/etc. Overall, we need to define some standard patterns for achieving the following behaviors:

- Track all resources needed by the server (HTTP server threads, outgoing RPC client instances, stateful background threads, etc.)
    - Block the acceptable of HTTP requests until all resources are 'ready'
    - Continously monitor resource health to ensure that the server can still accept additional work.
    - When a resource's task fails, we should stop the entire server.
- Gracefully shutdown all resources for server shutdown (e.g. on SIGINT / Ctrl-C)
    - Normally this process will must be staged with some resources performing cleanup before their dependencies e.g.
        - First put the RPC server into a lame duck state
        - Finish executing any in-progress RPCs
        - Maybe perform some leader transfer protocol or release locks.
        - Shutdown resources like outgoing RPC clients.
        - Finally `exit()` the program cancelling any other untracked outstanding work.

Risks:

- `executor::spawn()` tasks that are not tracked may internally fail and not log or report their error to another owner task.
- Many tasks just have big `loop { ... }` statements for continously doing work and we want to prevent these from blocking server shutdown.

## Existing Patterns

### Caller Managed Lifecycle

```
let worker = Worker::create();

let worker_task = executor::spawn(worker.run());

// ...

worker.shutdown();

worker_task.join().await?;
```

Pros:

- Usage of a shutdown method can help to differentiate between graceful and abrupt stopping (though this is fairly generic and could be based on some deadline for stopping).
- Clear propagation of `run()` error to the caller
    - But caller is not notified if it ended up failing a long time before shutdown() was called
    - But caller may not have a good sense of what to do with the error

Cons:

- Caller must promise to call continue polling `run()` for Worker to work correctly.
    - `run()` may not be cancel safe so the client must avoid a
    - `run()` SHOULD NOT be invoked in parallel more than once 


### Worker Owned Task

```
struct Worker {
    background_thread: Task<()>,
}

let worker = Worker::create()?;

worker.do_stuff();

worker.shutdown().await?;

worker.wait_for_shutdown().await?;

```

This approach has similar problems to the first one except:

- The worker has more control over ensuring there is only one instance in existance, but
- The caller has less visibility into the state of the background tasks for upward propagation.


## Resource Abstraction

We as a client of a resource want to:

- Poll the current health/lifecycle state of the resource.
    - e.g. if we need to response to a `/healthz` request.
    - It would be useful for this state information to also have a rich explanation for debugging for why we are in the current state.
- Get notified when the state of a resource has changed.
    - e.g. once all tasks are done or one has failed, we want to exit the 
- Consolidate the states of many resources into one overall state to describe the system.
    - A service may have many background threads or internal workers like RPC connections.
    - These background threads/workers may similarly recursively have their own dependencies that need to be consolidated.
- Know when to start running shutdown logic.
    - e.g. via some cancellation propagation.


Option 1: Resource Trait

```
enum ResourceHealth {
    Loading,
    Ready,
    TemporaryFailure,
    PermanentFailure(Error),
    Done
}

// Resources that can be health tracked should implement this trait.
//
// NOTE: In the Resource's constructor/create method, it should take in a CancellationToken from its owner.
trait ServiceResource {
    async fn resource_state(&self) -> ResourceHeath;

    // Creates an event listener to wait for changes to resource_state
    async fn resource_state_listener() -> Box<dyn Listener>;
}


// Resources which consist of many children will use this.
// Implements ServiceResource
struct ResourceManager { ... }

let resources = ResourceManager::new("MyServe", cancellation_token); 

let http_client = Arc::new(http::Client::new(resources.child_cancellation_token()));
resources.add_dependency(http_client.clone());

// Eventually we will be done:
// This will allow dependencies to proceed with shutting down.
resources.mark_done();

```

The simplest form of resource will be a simple async background task that eventually returns a `Result<()>` when finished and has no concept of being done 'loading'.


Crate name:

- Will encompass standard resource definitions and multi-task bundling / execution.
- Effectively making us act as a process.

- `executor_multitask::ServiceResource`
- `executor_resource`
- `service_resource`



Some things to keep in mind:

- Sometimes need a pattern of 'let resource = Resource::new(); resource.add_dependency(x); resource.start();'
- When the outer most `Arc<dyn ServiceResource>` for a resource is dropped, the resource can assume no more dependents are going to be added so can cancel itself if there aren't any dependents.


- TODO: Need to log PermanentFailure state changes in some global location so that they are always captured.


Option 2: Resource Health Tracker

```
// Passed into the resource as part of its constructor.
trait ResourceHealthTracker {
    // The resource is responsible for calling this whenever the state changes.
    fn report_resource_state(&mut self, state: ResourceHealth);
    
    //
    fn cancellation_token(&self) -> Box<dyn CancellationToken>;
}
```




Tasks are sensitive to whether or not they are allowed to be cancelled.

Some tasks will care about doing their own shut down logic and others will just have a `loop { ... }` which is ok to stop.

Another output would be to 