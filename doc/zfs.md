
`sudo zpool import -a`
or
`sudo zpool import -f tank`

`zfs set keylocation=file:///path/to/key <nameofzpool>/<nameofdataset>`

`zfs load-key -a`

`zfs mount tank/data`