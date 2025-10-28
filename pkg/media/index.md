

REcording Micr

https://trac.ffmpeg.org/wiki/Capture/PulseAudio

pactl list short sources

pactl set-default-source alsa_input.usb-Shure_Inc_Shure_MV7__MV7__9-b4e25ffce30d955494b292618bd701a7-01.mono-fallback


cd /mnt/nas/tank/data/projects/Voron-0/0018_Bed/Audio

ffmpeg -f pulse -i alsa_input.usb-Shure_Inc_Shure_MV7__MV7__9-b4e25ffce30d955494b292618bd701a7-01.mono-fallback -ac 1 -ar 48000 electronics_install.flac


```
for f in *.webm; do ffmpeg -i "$f" -vf "crop=trunc(iw/2)*2:trunc(ih/2)*2" -c:v libx264 -crf 22 -preset slow -pix_fmt yuv420p -c:a aac -b:a 128k "${f%.webm}.mp4"; done
```