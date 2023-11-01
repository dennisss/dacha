
Install Ubuntu 22.04.3

- 1 GB EFI partition
- 4 GB EXT4 /boot partition
- Rest goes to encrypted (LUKS) partition with BTRFS

Install via snap store

- GIMP
- VSCode

Install Chrome by downloading the .deb from Google's site.

Install vanilla Gnome

Install packages

- `sudo apt install vlc build-essential git ssh vim curl`

Under Gnome settings > Sharing, turn off Remote login (to disable local ssh server)

Install firewall:
- 'Firewall Configuration' in software store (aka gufw)
	- Don't use the snap
	- In general configure for always public (with incoming always blocked)

sudo apt install tmux

`sudo apt install default-jdk`


libreoffice
	- Install as the snap

evince
	- install as a snap

Disable `Continue running background apps when Google Chrome is closed` in chrome to allow it to more gracefully reboot

Gnome Date & Time settings
- Enable auto-time-zone
- Change to AM/PM time format

Installing python3 stuff now:
- See https://github.com/pypa/pipenv/issues/2122#issuecomment-386267993
- Just to get pip3
	- `sudo apt-get install python3-pip python3-setuptools`
- `pip3 install pipenv`
- Now restart (mainly so that ~/.local/bin gets into the path)
- pip3 should now be installed in ~/.local/python3


`sudo apt install gitg`

`sudo apt install default-jdk`


Install docker from their registry
- See https://www.digitalocean.com/community/tutorials/how-to-install-and-use-docker-on-ubuntu-22-04

Don't forget to do `usermod -a -G docker dennis` 

sudo snap install kubectl --classic

- Up the file handlers count
	- https://code.visualstudio.com/docs/setup/linux#_visual-studio-code-is-unable-to-watch-for-file-changes-in-this-large-workspace-error-enospc
		- Edit `/etc/sysctl.conf` to include `fs.inotify.max_user_watches=524288`

- 'rustup', 'nvm'

Install google cloud SDK via package (don't use snap):
- https://cloud.google.com/sdk/docs/install#deb
- At the end 'sudo apt install google-cloud-sdk-pubsub-emulator kubectl'
- Need to connect kubectl to gcloud
    - `gcloud container clusters get-credentials cluster-1`


Append `noatime` to the options for all filesystems in `/etc/fstab`

- TODO: Do this for all of my machines.
