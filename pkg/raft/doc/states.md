Server States
=============

For the most part, every server is going to be in either a Follower, Candidate, or Leader state similar to the typical Raft specification so most of the common details of the behaviors are skipped here. Rather, this documentation mainly focuses on the some of the important details we rely on that aren't necessarily clearly defined elsewhere.

Servers can also be more broadly in one of the following two states:
- `Member`: A server that votes in the cluster and can become a leader
- `Learner`: A server that behaves mostly like a follower but does not vote and can not become a leader
	- We will also have a `pending` flag on learners to mark learners which want to become full fledged members. The active leader of the cluster will decide when the learner will get promoted and will do the promoting


Below are behaviors primarily describing voting member behaviors


Follower
--------
- Upon becoming a follower, a random election timeout will be chosen and the follower will wait for this amount of time before starting an election
- If a follower is the only voting member in the cluster, then it can immediately start an election (and win it) without waiting for the timeout to end
- If a follower knows of a commit_index that is higher than its last_log_index, then it will remain in the follower state indefinately without starting its own elections until another leader is elected
- If a follower receives a RequestVote and is able to (re-)accept it, then it will reset its local election timeout

Leader Behavior
---------------

- Upon being elected, a leader will always immediately send some kind of AppendEntries heartbeat to its followers
	- In some cases as listed below it may also choose to create a no-op operation to progress the state machine

- By the Leader Completeness property, the leader should have all commited operations
- When a leader replicates its log using AppendEntries, if a follower responds with a success, it will also respond with the last index in that follower's log
	- If the follower's log is longer than the leader's log, then the leader will execute a no-op operation in its term (this should only be necessary if no other operations have been executed in the leader's term yet)
		- This may be necessary in the case that a previous leader crashed or was demoted and has left hanging uncommited entries on some machines
		- By applying a no-op, this will truncate the follower's log immediately and will ensure that any execute() operations on that machine immediately finish rather than stalling in the case of no other operations progressing the log
	- TODO: It may also be useful to get back a last_log_index from the RequestVote response to expedite this process

- Upon being elected leader, a leader may also immediately execute a no-op in the case that it notices that it locally has any uncommitted entries in its log

