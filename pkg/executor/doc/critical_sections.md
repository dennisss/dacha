# Mutex Design

TODO: The key here is that public APIs should always be cancel safe.

## Overview

In this doc, we'll describe the design of the `Mutex` primitive used in this project.

- Compared to the `std::sync::Mutex`, it supports async waiting for a mutex to lock and running async operations  
- Compared to other Rust async mutex implementations, it guarantees that undefined state changes are not observed and cause poisoning.

## Critical Sections and Safety

We define a critical section as a region of code that must be executed to completion without cancellation, failures, or interruption by concurrent threads accessing the same data. For the most part, these show up when we wrap some shared data in a mutex lock to ensure exactly one thread can mutate it at a time and writes have a well defined serial order.

Consider the following example with some `Mutex<State>` stored in `state`:

```
let mut guard = state.lock().unwrap();
guard.a -= 1;
// X
guard.b += 1;
```

If the program is cancelled at line `X`, then the state will be in an undefined state as the mutation to `b` hasn't taken place but the mutation to `a` has. Depending on the application logic, this may be desirable (e.g. if transferring money from one bank account to another).

In the case that we do believe that the state guarded be a mutex has switched to an undefined, we want to signal this to all users (e.g. by poisoning the mutex and preventing future access).

## Goal

We want to define some `Mutex` primitive that is able to track cancelled critical sections and in term poisons the lock in response. Further more, we want to support async operations with locks: either async waiting for a lock to become locked or while a lock is being held, perform other async operations before releasing the lock.

## Hazards

### Future Cancellation

```
let mut guard = state.lock().await;
guard.a -= 1;
log.write("...").await;  // X
guard.b += 1;
```

### Panic

```
let mut guard = state.lock().await;
guard.a -= 1;
log.write("...").await;  // X
guard.b += 1;
```

### 'Read-only' Access to Data

```
let guard = state.lock().await;
guard.a -= 1;
log.write("...").await;  // X
guard.b += 1;
```

### Error Progagation

=> Leave this as the user's responsibility as there is no ergonomic and fast way to handle this.

## Design

Raw interface:

```
let state = Mutex::new(123);

let guard = state.lock().await?; // May error out if poisoned.

let mut section = guard.enter();

// Do changes to the state here.

section.exit();
```


RCU pattern should be supported

Macros:

Need to:
- support sync/async locking
- sync/async inner function
- Poison unwrap.


```

lock!(state = self.shared.processing_state.lock().await?, {
    state.shutting_down = true;
});

lock!(lock self.state as state, {
    let value = a.await;
    *state = value;
});
```

Lock

- Intercept normal drops and 

### Avoiding Future Cancellations

Note that we want to 

Future cancellation will trigger


Critical libraries that must be cancel safe:
- RPC client and libraries
    - Bubbles down to HTTP and TLS
- Databases
    - Client side and server side code
- Everything else should ideally use DB transactions for state which enable cancel safety through rollback when transaction are not committed.



Handling file I/O
- flush needs to continuously return errors if there is cancellation or an initial error.
- writes to the end of the file should ideally poison the state such that future writes also error out.
    - Use for any type of append-only log to ensure that additional writes don't get mistaken as successes.
