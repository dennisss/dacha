

Step 1: Connect ZFS drives

Step 2: `sudo zpool import tank`
- Should show up under `/tank`

Step 3: Backup!

```
restic -r /tank/restic backup /home/dennis --exclude-file=doc/machines/restic_excludes.txt
```

$ restic -r /srv/restic-repo backup --one-file-system /


Loading the encrpyted dataset:

```
sudo zfs load-key -L file:///home/dennis/Documents/pass/data.key tank/data
sudo zfs mount tank/data
```


Later:

```
sudo zpool export tank
```

TODO: Need to support preventing Linux sleep while drives are active.