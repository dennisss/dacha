use crate::proto::consensus::*;
use crate::proto::consensus_state::*;

/*
    NOTE: LogCabin adds various small additions offer the core protocol in the paper:
    - https://github.com/logcabin/logcabin/blob/master/Protocol/Raft.proto#L126
    - Some being:
        - Full generic configuration changes (not just for one server at a time)
        - System time information/synchronization happens between the leader and followers (and propagates to the clients connected to them)
        - The response to AppendEntries contains the last index of the log on the follower (so that we can help get followers caught up if needed)

    Types of servers in the cluster:
    - Voting members : These will be the majority of them
    - Learners : typically this is a server which has not fully replicated the full log yet and is not counted towards the quantity of votes
        - But if it is sufficiently caught up, then we may still send newer log entries to it while it is catching up

    - XXX: We will probably not deal with these are these are tricky to reason about in general
        - VoteFor <- Could be appended only locally as a way of updating the metadata without editing the metadata file (naturally we will ignore seeing these over the wire as these will )
            - Basically we are maintaining two state machines (one is the regular one and one is the internal one holding a few fixed values)
        - ObserveTerm <- Whenever the

    - The first entry in every single log file is a marker of what the first log entry's index is in that file
        - Naturally some types of entries such as VoteFor will not increment the

    - Naturally next step would be to ensure that the main Raft module tries to stay at near zero allocations for state transitions
*/

enum ServerRole {
    Member,
    PendingMember,
    Learner,
}

/*
    TODO: Other optimization
    - For very old well commited logs, a learner can get them from a follower rather than from the leader to avoid overloading the leader
    - Likewise this can be used for spreading out replication if the cluster is sufficiently healthy
*/
