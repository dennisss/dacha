# PC Backup

This is how I backup my main PC to my [NAS](./index.md). I've already set up a [Restic](https://restic.readthedocs.io/en/latest/index.html) repository at `/zfs/tank/restic` on the NAS owned by `cluster-user` with `700` permissions.

To allow restic to connect with an SSH key, we need to add the following to `~/.ssh/config` (such restic can't take the SSH key as an argument):

```
Host nas
    Hostname 10.2.0.1
    Port 22
    User cluster-user
    IdentityFile "~/.ssh/id_cluster"
    IdentitiesOnly yes
```

To verify there are no massive files in my home directory, I run:

```
du -h -t 1G ~
```

Any big files are better to be maintained in a more structured way on my NAS.

Then to run the backup (running from the root of the dacha repository):

```
restic -r sftp:nas:/zfs/tank/restic backup $HOME --exclude-file=pkg/cluster/machines/nas/restic_excludes.txt
```

NOTE: My current restic_excludes file assumes that the dacha repo is located at `$HOME/workspace/dacha` to properly exclude compiled files.

A quick command reference:

```
# List snapshots in the repository
restic -r sftp:nas:/zfs/tank/restic snapshots

# Pruning data
restic -r sftp:nas:/zfs/tank/restic prune

```
