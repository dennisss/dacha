
## Useful commands:

Record microphone sound:

```

ffmpeg -f pulse -i alsa_input.usb-Shure_Inc_Shure_MV7__MV7__9-b4e25ffce30d955494b292618bd701a7-01.mono-fallback -ac 1 -ar 48000 audio.flac
```


Convert WebM to MP4:

```
I am using the following command to convert a single webm in my directory to mp4 but it is very slow. ffmpeg is logging frame counts in the 10s of thousands but the video is like 10 seconds long.

for f in *.webm; do ffmpeg -i "$f" -vf "crop=trunc(iw/2)*2:trunc(ih/2)*2" -vsync vfr -c:v libx264 -crf 22 -preset slow -pix_fmt yuv420p -c:a aac -b:a 128k "${f%.webm}.mp4"; done
```





## Misc


REcording Micr

https://trac.ffmpeg.org/wiki/Capture/PulseAudio

pactl list short sources

pactl set-default-source alsa_input.usb-Shure_Inc_Shure_MV7__MV7__9-b4e25ffce30d955494b292618bd701a7-01.mono-fallback



