/*

Condvar:
- Stores list of tasks that want to be woken up and whether or not they have been woken up
- To wake a task, mark it as waking and enqueue the task in the executor.


Suppose we have one task


Other functions we need:

- executor::spawn() -> JoinHandle
- executor::current_task() -> Task { entry }
    - We can call .wake() on a


I may call notify_one() which will request at least one

If one event is lost, we need to ask someone else.

*/

struct Condvar {}
