# Home Hub

This is an application interfaces with an attached Elgato Stream Deck to perform the following (currently hardcoded) functions based on which buttons a user presses:

- Switch between different display inputs on a connected monitor (via HDMI DDC).
- Turn on/off Hue light groups.

## User Guide

As a pre-requisite, we assume that you have a Raspberry Pi that has been setup in a [cluster](../../container/index.md) and has attached:

- A Stream Deck via USB
- A monitor via HDMI
    - Plug into the left HDMI port (nearest the USB-C port) so that it is exposed as `/dev/i2c-20`. 

First you need to obtain a Hue bridge username (a secret password to control the lights). Run the following and follow the instructions:

```
cargo run --bin hue -- create_user \
    --application_name=dacha --device_name=home_hub
```

The above command should print a secret user name and then you can save in the metastore for the application to use:

```
cargo run --bin home_hub_config -- \
    --config_object=home_hub_config \
    --set_config="hue_user_name: \"[INSERT_USER_NAME]\""
```

Lastly start the job running the application:

```
cargo run --bin cluster -- start_job pkg/app/home_hub/config/main.job
```

Note that this assumes that exactly one machine has a stream deck attached.

## Notes

- Icons are derived from Font Awesome (Free). See https://github.com/FortAwesome/Font-Awesome for licensing and original work.
- Each icon is 72 x 72 pixel image per button (180 degree flipped)
    - NOTE: These must be non-progressive JPEGs for the Stream Deck to work with them.

