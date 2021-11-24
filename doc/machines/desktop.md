
Install Ubuntu 20.04 LTS

Install vanilla gnome

Key things copied over from last machine
- ~/.gitconfig ~/.ssh

Install chrome via .deb on website

Small packages to install via apt:
- vim, curl, git, gnome-backgrounds

Snap packages:
- VLC, VS Code
- Psensors

Install dev
- 'rustup', 'nvm'

Install google cloud SDK via package (don't use snap):
- https://cloud.google.com/sdk/docs/install#deb
- At the end 'sudo apt install google-cloud-sdk-pubsub-emulator kubectl'
- Need to connect kubectl to gcloud
    - `gcloud container clusters get-credentials cluster-1`


Needed for dacha:
- `sudo apt install libudev-dev libusb-1.0-0-dev cmake libasound2-dev libxkbcommon-dev xorg-dev`

Installing docker as mentioned here:
- https://docs.docker.com/engine/install/ubuntu/

Follow this for how to get docker working as non-root:
- https://docs.docker.com/engine/install/linux-postinstall/

Installing Bazel
- https://docs.bazel.build/versions/4.2.1/install-ubuntu.html#install-on-ubuntu