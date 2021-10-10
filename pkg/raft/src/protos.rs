use crate::proto::consensus::*;
use crate::proto::consensus_state::*;

/*
    NOTE: When two servers first connect to each other, they should exchange cluster ids to validate that both of them are operating in the same namespace of server ids

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

    - Modes of log compaction
        - Snapshotting
        - Compression
            - Simply doing a gzip/snappy of the log
        - Evaluation (for lack of a better work)
            - Detect and remove older operations which are fully overriden in effect by a later operation/command
            - This generally requires support from the StateMachine implementation in being able to efficiently produce a deduplication key for every operation in order to allow for linear scanning for duplicates

    - XXX: We will probably not deal with these are these are tricky to reason about in general
        - VoteFor <- Could be appended only locally as a way of updating the metadata without editing the metadata file (naturally we will ignore seeing these over the wire as these will )
            - Basically we are maintaining two state machines (one is the regular one and one is the internal one holding a few fixed values)
        - ObserveTerm <- Whenever the

    - The first entry in every single log file is a marker of what the first log entry's index is in that file
        - Naturally some types of entries such as VoteFor will not increment the

    - Naturally next step would be to ensure that the main Raft module tries to stay at near zero allocations for state transitions
*/

// impl Default for Metadata {
//     fn default() -> Self {
//         Metadata {
//             current_term: 0,
//             voted_for: None,
//             commit_index: 0,
//         }
//     }
// }

enum ServerRole {
    Member,
    PendingMember,
    Learner,
}

// impl Default for ConfigurationSnapshot {
//     fn default() -> Self {
//         ConfigurationSnapshot {
//             last_applied: 0,
//             data: Configuration::default(),
//         }
//     }
// }

pub struct Snapshot {
    // The group_id should probably also be part of this?
    pub config: Configuration,
    pub state_machine: Vec<u8>, // <- This is assumed to be internally parseable by some means
}

/*
    TODO: Other optimization
    - For very old well commited logs, a learner can get them from a follower rather than from the leader to avoid overloading the leader
    - Likewise this can be used for spreading out replication if the cluster is sufficiently healthy

*/

/*
    How we will generalize snapshots:
    ->

    The good news is that we hold the configuration and the state machine to be pretty orthogonal

    Generalizing the snapshot process:
    -> Step 1:
        -> Snapshot transferred in memory and retained in memory
        -> State machine should support
        -> The InstallSnapshot handler may emit a complete chunk once the state machine needs to be restored
            ->

*/

pub struct AddServerRequest {}
