# Debian Repository Tools


## Format

As mentioned [here](https://wiki.debian.org/DebianRepository), a repository contains many 'releases' (like `wheezy`, `bullseye`, ...). A 'release' is also known as a 'distribution'. Each release may have multiple components (e.g. `main`, `contrib`, `non-free`).

The inner format is described [here](https://wiki.debian.org/DebianRepository/Format). Each repository is described by a `uri`  which points to a HTTP directory that serves as the root of the repository. Often times the directory is named `debian` (e.g. `http://deb.debian.org/debian`).

## Source List

In Debian Linux, repositories are listed in the `/etc/apt/sources.list*` files.

Each line is of the form:

```
deb[-src] [url] [release] [component]*
```

In Raspberry Pi these contain:

```
# /etc/apt/sources.list

deb http://deb.debian.org/debian bullseye main contrib non-free
deb http://security.debian.org/debian-security bullseye-security main contrib non-free
deb http://deb.debian.org/debian bullseye-updates main contrib non-free
# Uncomment deb-src lines below then 'apt-get update' to enable 'apt-get source'
#deb-src http://deb.debian.org/debian bullseye main contrib non-free
#deb-src http://security.debian.org/debian-security bullseye-security main contrib non-free
#deb-src http://deb.debian.org/debian bullseye-updates main contrib non-free

# /etc/apt/sources.list.d/raspi.list

deb http://archive.raspberrypi.org/debian/ bullseye main
# Uncomment line below then 'apt-get update' to enable 'apt-get source'
#deb-src http://archive.raspberrypi.org/debian/ bullseye main
```

## Archive

The root directory located at the `url` contains the following files:

- `./dists/[release]/`: A 'distribution directory'
    - `./Release`: Contains a list of index files and their hashes. Paths are relative to the distribution directory.
    - `./Release.gpg` : Signature for `./Release`
    - `./InRelease`: Inline signed version of `./Release`
    - `./[component_name]`
        - `./binary-[arch]/Packages` : Typical location of one of the binary index files
        - `./source/Sources` : Typical location of one of the source index files.
        - `./Contents-[arch]`: Table mapping file names (relative to Linux installation root '/') to the full package name (space separated table with two columns).
        - `./Translation`
- `./pool/[component_name]/[prefix]/[package_name]/`
    - Typical location where package data is stored
    - `prefix` is usually the first letter of the package name (or `libx` where `x` is the first letter after a `lib` prefix).



The 'Release', 'Packages', and 'Sources' indexes can be compressed based on their file extension:

- (no extension) : Not compressed
- `.xz` : XZ compression (must be supported by clients). Should normally be the main format provided by servers.
- `.gz` : GZip compression
- ..


The index files are 'control files' defined [here](https://www.debian.org/doc/debian-policy/ch-controlfields.html#syntax-of-control-files).


## Notes

- Raspberry Pi images initialized with [debootstrap](https://linux.die.net/man/8/debootstrap)
- Debian repository keys are [here](https://ftp-master.debian.org/keys.html)
- Raspberry Pi Archive Key is [here](https://archive.raspberrypi.org/debian/raspberrypi.gpg.key)
- OpenPGP signatures are documented [here](https://www.rfc-editor.org/rfc/rfc4880)


## Old

/*

Also need to know the release distro and the GPG keys


Main target is to download the repository

TODO: When we build stuff, use a GNU debug link? https://stackoverflow.com/questions/46197810/separating-out-symbols-and-stripping-unneeded-symbols-at-the-same-time
*/
