
Setting up gadget mode:
- https://learn.adafruit.com/turning-your-raspberry-pi-zero-into-a-usb-gadget/serial-gadget

Need pulseaudio drivers for bluetooth devices:
- https://unix.stackexchange.com/questions/258074/error-when-trying-to-connect-to-bluetooth-speaker-org-bluez-error-failed


How to connect to bluetooth via CLI:
- https://www.cnet.com/how-to/how-to-setup-bluetooth-on-a-raspberry-pi-3/
- or https://computingforgeeks.com/connect-to-bluetooth-device-from-linux-terminal/

Bluetooth address of headphones: CC:98:8B:9A:22:B4

```
Device CC:98:8B:9A:22:B4 (public)
	Name: LE_WH-1000XM3
	Alias: LE_WH-1000XM3
	Class: 0x00240404
	Icon: audio-card
	Paired: yes
	Trusted: yes
	Blocked: no
	Connected: no
	LegacyPairing: no
	UUID: Vendor specific           (00000000-deca-fade-deca-deafdecacaff)
	UUID: Headset                   (00001108-0000-1000-8000-00805f9b34fb)
	UUID: Audio Sink                (0000110b-0000-1000-8000-00805f9b34fb)
	UUID: A/V Remote Control Target (0000110c-0000-1000-8000-00805f9b34fb)
	UUID: A/V Remote Control        (0000110e-0000-1000-8000-00805f9b34fb)
	UUID: Handsfree                 (0000111e-0000-1000-8000-00805f9b34fb)
	UUID: PnP Information           (00001200-0000-1000-8000-00805f9b34fb)
	UUID: Vendor specific           (7b265b0e-2232-4d45-bef4-bb8ae62f813d)
	UUID: Vendor specific           (81c2e72a-0591-443e-a1ff-05f988593351)
	UUID: Vendor specific           (931c7e8a-540f-4686-b798-e8df0a2ad9f7)
	UUID: Vendor specific           (96cc203e-5068-46ad-b32d-e316f5e069ba)
	UUID: Vendor specific           (b9b213ce-eeab-49e4-8fd9-aa478ed1b26b)
	UUID: Vendor specific           (f8d1fbe4-7966-4334-8024-ff96c9330e15)
	Modalias: usb:v054Cp0CD3d0422
```
	
To Start the daemon as the current user
`pulseaudio-d`

Seems to only work when using bluealsa though:
https://www.raspberrypi.org/forums/viewtopic.php?t=222527


bluealsa:DEV=CC:98:8B:9A:22:B4,PROFILE=a2dp

/home/pi/.asoundrc
```
defaults.bluealsa.service "org.bluealsa"
defaults.bluealsa.device "CC:98:8B:9A:22:B4"
defaults.bluealsa.profile "a2dp"
defaults.bluealsa.delay 10000
```

Need to make a bluealsa service eventually:
https://askubuntu.com/questions/1197072/sending-audio-to-bluetooth-speaker-with-bluealsa

`SDL_AUDIODRIVER="alsa" AUDIODEV="bluealsa" ffplay -nodisp test.mp3`

/proc/asound/cards


Usage of the device:
- https://developer.ridgerun.com/wiki/index.php/How_to_use_the_audio_gadget_driver

aplay -D plug:hw:1 audio_file.wav


Loop one to the other:
    arecord -D plug:hw:1 --buffer-size=5 -f cd - | aplay --buffer-size=5 -D bluealsa -

Outstanding concerns:
- Verify that we can send over compressed audio to the headphones.
