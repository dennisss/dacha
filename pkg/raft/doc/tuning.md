
Size of the log must be > memtable size to ensure that we snapshot the memtable to disk before running out of log space.


- But, log must be << state machine to be scalable as log is in memory.
- Everything in the log will be in the mem table.
- Let's set the EmbeddedDB as follows:
    - Key Range Size: [256 MiB, 1024 MiB] (average 512MB)
    - Memtable size: 32 MiB
    - Ratio is ~1:16 of log/memtable (in RAM) to on disk
    - As long as <1/16th of the database changes in 10 minutes, should be safe to re-sync from logs.
    - Log will hold things in 16 MiB segments
    - Always run discard() for committed entries
        - log is responsible for figuring out when to actualyl discard stuff.



When should a log discard entries that are fully applied to the state machine?

- For now algotithm is to keep one extra log segment