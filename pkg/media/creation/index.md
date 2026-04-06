# Media Creation Tools

This directory contains scripts and documentation used for creating content like YouTube videos.

## Animations

The `./animations` directory contains JavaScript canvas based animations that can be rendered into MP4 videos to embed in a larger video.

To preview the animation, run:

```
python3 -m http.server 9000
```

in the root workspace directory and navigate to `http://localhost:9000/pkg/media/creation/animation/animation.html` in a web browser like Chrome. Currently which of the animations is being viewed is hardcoded in the html file's code.

Code for individual animations is in the `./animations/timelines` directory:

- A single `Timeline` object equals one video.
- In the browser and in the code, rendering is done at a resolution of 960 x 540.
    - (all the X/Y coordinatee units in the code are relative to this size)
- Final rendering to MP4 happens at a resolution of 3840 x 2160

To render an animation to an MP4 file:

```
nvm use
node -r esm pkg/media/creation/animation/js/node.js
```

## Davinci Resolve Settings:

- Audio Normalization:
  - Just select all the dialogue and do `Normalize Audio Levels`, `YouTube`, `Independent`
- Apply Music:
  - Volume: -35


## Useful commands:

**Record microphone sound:**

```

ffmpeg -f pulse -i alsa_input.usb-Shure_Inc_Shure_MV7__MV7__9-b4e25ffce30d955494b292618bd701a7-01.mono-fallback -ac 1 -ar 48000 0001_intro.flac
```

**Convert WebM to MP4:**

```
for f in *.webm; do \
  ffmpeg -i "$f" \
  -vf "crop=trunc(iw/2)*2:trunc(ih/2)*2" \
  -vsync vfr \
  -c:v libx264 -crf 22 -preset fast -pix_fmt yuv420p \
  -c:a aac -b:a 128k \
  "${f%.webm}.mp4"; \
done
```


**Speed up a video by 30x**

```
ffmpeg -i input.mp4 -vf "select='not(mod(n,30))',setpts=N/30/TB" -an -c:v libx264 -crf 18 -preset slow output.mp4
ffmpeg -hwaccel cuda -i input.mp4 -vf "select='not(mod(n,30))',setpts=N/30/TB" -an -c:v h264_nvenc -preset p7 -cq 19 -rc vbr output.mp4


ffmpeg -i C1221.MP4 -vf "select='not(mod(n,30))',setpts=N/30/TB" -an -c:v libx264 -crf 18 -preset slow C1221_30x.mp4
```

**Resize a window to an exact size**

```
# Find window id
wmctrl -l

# Resize it
wmctrl -i -r 0x01400103 -e 0,0,0,1920,1080
```



## Misc


REcording Micr

https://trac.ffmpeg.org/wiki/Capture/PulseAudio

pactl list short sources

pactl set-default-source alsa_input.usb-Shure_Inc_Shure_MV7__MV7__9-b4e25ffce30d955494b292618bd701a7-01.mono-fallback



